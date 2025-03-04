use std::borrow::Borrow;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use crate::internal::*;
use crate::model::order::eval_order_for_nodes;
use crate::model::{ModelImpl, OutletId, TensorInfo};

#[derive(Debug, Default)]
pub struct SessionState {
    pub inputs: HashMap<usize, Arc<Tensor>>,
    pub known_stream_len: Option<usize>,
    pub tensors: HashMap<String, Tensor>,
}

#[derive(Debug, Clone)]
pub struct SimplePlan<TI, O, M>
where
    TI: TensorInfo + Clone + 'static,
    O: Debug + Display + AsRef<dyn Op> + AsMut<dyn Op> + Clone + 'static,
    M: Borrow<ModelImpl<TI, O>>,
{
    pub model: M,
    pub outputs: Vec<OutletId>,
    pub order: Vec<usize>,
    pub flush_lists: Vec<TVec<usize>>,
    _casper: PhantomData<(TI, O)>,
}

impl<TI, O, M> SimplePlan<TI, O, M>
where
    TI: TensorInfo + Clone + 'static,
    O: Debug + Display + AsRef<dyn Op> + AsMut<dyn Op> + Clone + 'static,
    M: Borrow<ModelImpl<TI, O>>,
{
    /// This contructor returns a plan that will compute all the model default outputs in one pass.
    pub fn new(model: M) -> TractResult<SimplePlan<TI, O, M>> {
        let outputs = model.borrow().output_outlets()?.iter().cloned().collect::<Vec<OutletId>>();
        Self::new_for_outputs(model, &outputs)
    }
    /// This contructor returns a plan that will compute the specified output.
    pub fn new_for_output(model: M, output: OutletId) -> TractResult<SimplePlan<TI, O, M>> {
        Self::new_for_outputs(model, &[output])
    }
    /// This contructor returns a plan that will compute all specified outputs in one pass.
    pub fn new_for_outputs(model: M, outputs: &[OutletId]) -> TractResult<SimplePlan<TI, O, M>> {
        let inputs = model.borrow().input_outlets()?.iter().map(|n| n.node).collect::<Vec<usize>>();
        let outputs_nodes = outputs.iter().map(|n| n.node).collect::<Vec<usize>>();
        let order = eval_order_for_nodes(model.borrow().nodes(), &inputs, &outputs_nodes)?;
        let mut values_needed_until_step = vec![0; model.borrow().nodes().len()];
        for step in 0..order.len() {
            for i in &model.borrow().node(order[step]).inputs {
                values_needed_until_step[i.node] = step;
            }
        }
        for o in outputs.iter() {
            values_needed_until_step[o.node] = order.len();
        }
        let mut flush_lists: Vec<TVec<usize>> = vec![tvec!(); order.len() + 1];
        for (node, &flush_at) in values_needed_until_step.iter().enumerate() {
            if flush_at != 0 {
                flush_lists[flush_at].push(node)
            }
        }
        Ok(SimplePlan {
            model,
            order,
            flush_lists,
            outputs: outputs.to_vec(),
            _casper: PhantomData,
        })
    }

    pub fn run(&self, inputs: TVec<Tensor>) -> TractResult<TVec<Arc<Tensor>>> {
        let mut state = SimpleState::new(self)?;
        state.run(inputs)
    }

    pub fn model(&self) -> &ModelImpl<TI, O> {
        self.model.borrow()
    }
}

#[derive(Debug)]
pub struct SimpleState<TI, O, M, P>
where
    TI: TensorInfo + Clone + 'static,
    O: Debug + Display + AsRef<dyn Op> + AsMut<dyn Op> + Clone + 'static,
    M: Borrow<ModelImpl<TI, O>>,
    P: Borrow<SimplePlan<TI, O, M>>,
{
    plans: Vec<P>,
    pub states: Vec<Option<Box<dyn OpState>>>,
    pub session_state: SessionState,
    pub values: Vec<Option<TVec<Arc<Tensor>>>>,
    _phantom: PhantomData<(M, TI, O)>,
}

impl<TI, O, M, P> Clone for SimpleState<TI, O, M, P>
where
    TI: TensorInfo + Clone + 'static,
    O: Debug + Display + AsRef<dyn Op> + AsMut<dyn Op> + Clone + 'static,
    M: Borrow<ModelImpl<TI, O>>,
    P: Borrow<SimplePlan<TI, O, M>> + Clone,
{
    fn clone(&self) -> SimpleState<TI, O, M, P> {
        let states = self
            .states
            .iter()
            .map(|opt: &Option<Box<dyn OpState>>| -> Option<Box<dyn OpState>> {
                opt.as_ref().map(|b| ::objekt::clone_box(&**b))
            })
            .collect();
        SimpleState {
            plans: self.plans.clone(),
            states,
            session_state: SessionState::default(),
            values: self.values.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<TI, O, M, P> SimpleState<TI, O, M, P>
where
    TI: TensorInfo + Clone + 'static,
    O: Debug + Display + AsRef<dyn Op> + AsMut<dyn Op> + Clone + 'static,
    M: Borrow<ModelImpl<TI, O>>,
    P: Borrow<SimplePlan<TI, O, M>> + Clone,
{
    pub fn new(plan: P) -> TractResult<SimpleState<TI, O, M, P>> {
        Self::new_multiplan(vec![plan])
    }

    pub fn new_multiplan(plans: Vec<P>) -> TractResult<SimpleState<TI, O, M, P>> {
        let values = vec![None; plans[0].borrow().model.borrow().nodes().len()];
        let mut session = SessionState::default();
        let model = plans[0].borrow().model();
        let states = model
            .nodes()
            .iter()
            .map(|n: &BaseNode<TI, O>| n.op().state(&mut session, n.id))
            .collect::<TractResult<_>>()?;
        Ok(SimpleState { plans, states, session_state: session, values, _phantom: PhantomData })
    }

    /// Reset wires state.
    pub fn reset_wires(&mut self) -> TractResult<()> {
        self.values.iter_mut().for_each(|s| *s = None);
        Ok(())
    }

    /// Reset wires state.
    pub fn reset_op_states(&mut self) -> TractResult<()> {
        let &mut SimpleState { ref plans, ref mut session_state, ref mut states, .. } = self;
        *states = plans[0]
            .borrow()
            .model()
            .nodes()
            .iter()
            .map(|n| n.op().state(session_state, n.id))
            .collect::<TractResult<_>>()?;
        Ok(())
    }

    pub fn run(&mut self, inputs: TVec<Tensor>) -> TractResult<TVec<Arc<Tensor>>> {
        self.run_plan(inputs, 0)
    }

    pub fn run_plan(
        &mut self,
        inputs: TVec<Tensor>,
        plan: usize,
    ) -> TractResult<TVec<Arc<Tensor>>> {
        let mut result = tvec!();
        {
            self.set_inputs(inputs)?;
            let &mut SimpleState {
                ref plans,
                ref mut session_state,
                ref mut states,
                ref mut values,
                ..
            } = self;
            let plan = plans[plan].borrow();
            let model = plan.model().borrow();
            for (step, n) in plan.order.iter().enumerate() {
                let node = model.node(*n);
                trace!("Running step {}, node {}", step, node);
                let mut inputs: TVec<Arc<Tensor>> = tvec![];
                for i in &node.inputs {
                    trace!("  use input {:?}", i);
                    let prec_node = model.node(i.node);
                    let prec = values[i.node].as_ref().ok_or_else(|| {
                        format!("Computing {}, precursor {} not done:", node, prec_node)
                    })?;
                    inputs.push(prec[i.slot].clone().into())
                }

                for flush in &plan.flush_lists[step] {
                    trace!("  flushing node {} {}", flush, node);
                    values[*flush] = None;
                }

                if cfg!(debug_assertions) {
                    let facts = model.node_input_facts(node.id)?;
                    if facts.len() != inputs.len() {
                        bail!(
                            "Evaluating {}: expected {} inputs, got {}",
                            node,
                            facts.len(),
                            inputs.len()
                        );
                    }
                    for (ix, (v, f)) in inputs.iter().zip(facts.iter()).enumerate() {
                        if f.to_tensor_fact().shape.is_concrete()
                            && f.to_tensor_fact().stream_info()?.is_some()
                        {
                            continue;
                        }
                        if let Err(e) = f.to_tensor_fact().unify(&v.clone().into()) {
                            bail!(
                                "Evaluating {}: input {:?}, expected {:?}, got {:?} ({})",
                                node,
                                ix,
                                f,
                                v,
                                e
                            );
                        }
                    }
                }

                let vs = match states[node.id] {
                    Some(ref mut state) => state.eval(session_state, node.op(), inputs),
                    None => node.op().as_stateless().expect("as_stateless").eval(inputs),
                }
                .chain_err(|| format!("Evaluating {}", node))?;

                if cfg!(debug_assertions) {
                    let facts = model.node_output_facts(node.id)?;
                    if facts.len() != vs.len() {
                        bail!(
                            "Evaluating {}: expected {} outputs, got {}",
                            node,
                            facts.len(),
                            vs.len()
                        );
                    }
                    for (ix, (v, f)) in vs.iter().zip(facts.iter()).enumerate() {
                        if node.outputs[ix].successors.len() == 0 {
                            continue;
                        }
                        if f.to_tensor_fact().shape.is_concrete()
                            && f.to_tensor_fact().stream_info()?.is_some()
                        {
                            continue;
                        }
                        if let Err(e) = f.to_tensor_fact().unify(&v.clone().into()) {
                            bail!(
                                "Evaluating {}: output {:?}, expected {:?}, got {:?} ({})",
                                node,
                                ix,
                                f,
                                v,
                                e
                            );
                        }
                    }
                }

                values[node.id] = Some(vs);
            }
            for output in &plan.outputs {
                result.push(values[output.node].as_ref().unwrap()[output.slot].clone())
            }
        }
        self.reset_wires()?;
        Ok(result)
    }

    pub fn set_inputs(&mut self, inputs: TVec<Tensor>) -> TractResult<()> {
        let SimpleState { ref plans, ref mut session_state, .. } = self;
        plans[0].borrow().model().input_outlets()?.iter().zip(inputs).for_each(|(input, t)| {
            session_state.inputs.insert(input.node, t.into());
        });
        Ok(())
    }

    pub fn set_input(&mut self, input: usize, t: Tensor) -> TractResult<()> {
        let id = self
            .model()
            .input_outlets()?
            .get(input)
            .ok_or_else(|| format!("Invalid input id for model ({}).", input))?
            .node;
        self.session_state.inputs.insert(id, t.into());
        Ok(())
    }

    pub fn take_outputs(&mut self) -> TractResult<Vec<Arc<Tensor>>> {
        let SimpleState { ref plans, ref mut values, .. } = self;
        let mut v = vec![];
        for o in plans[0].borrow().model().output_outlets()?.iter() {
            let vs = values[o.node].as_mut().ok_or_else(|| {
                format!(
                    "Outputs of {:?} are not computed",
                    &plans[0].borrow().model().nodes()[o.node]
                )
            })?;
            v.push(vs[o.slot].clone())
        }
        Ok(v)
    }

    pub fn set_values(&mut self, id: usize, values: TVec<Tensor>) -> TractResult<()> {
        self.values[id] = Some(values.into_iter().map(|t| t.into()).collect());
        Ok(())
    }

    pub fn set_value(&mut self, id: usize, value: Tensor) -> TractResult<()> {
        self.set_values(id, tvec!(value))
    }

    pub fn compute_one(&mut self, node: usize) -> TractResult<()> {
        let SimpleState { ref plans, ref mut session_state, ref mut values, .. } = self;
        let plan = plans[0].borrow();
        let nodes = plan.model().nodes();
        let node = &nodes[node];
        let mut inputs: TVec<Arc<Tensor>> = tvec![];
        for i in &node.inputs {
            let prec_node = &nodes[i.node];
            let prec = values[i.node]
                .as_ref()
                .ok_or_else(|| format!("Computing {}, precursor {} not done.", node, prec_node))?;
            inputs.push(prec[i.slot].clone().into())
        }
        let vs = match self.states[node.id] {
            Some(ref mut state) => state.eval(session_state, node.op(), inputs),
            None => node.op().as_stateless().unwrap().eval(inputs),
        }
        .map_err(|e| format!("Evaluating {}: {}", node, e))?;
        values[node.id] = Some(vs);
        Ok(())
    }

    pub fn compute_recursively(&mut self, node: usize) -> TractResult<&[Arc<Tensor>]> {
        let values = {
            let precs: Vec<usize> =
                self.model().nodes()[node].inputs.iter().map(|i| i.node).collect();
            for i in precs.into_iter() {
                if self.values[i].is_none() {
                    let _ = self.compute_recursively(i)?;
                }
            }
            let mut inputs: TVec<Arc<Tensor>> = tvec![];
            {
                let node = &self.model().nodes()[node];
                for i in &node.inputs {
                    inputs.push(self.values[i.node].as_ref().unwrap()[i.slot].clone().into())
                }
            }
            let Self { ref mut states, ref mut session_state, ref plans, .. } = self;
            let plan = plans[0].borrow();
            match states[node] {
                Some(ref mut state) => {
                    state.eval(session_state, plans[0].borrow().model().nodes()[node].op(), inputs)
                }
                None => {
                    plan.borrow().model().nodes()[node].op().as_stateless().unwrap().eval(inputs)
                }
            }
            .map_err(|e| format!("Evaluating {:?}: {:?}", node, e))?
        };
        self.values[node] = Some(values);
        Ok(&*self.values[node].as_ref().unwrap())
    }

    pub fn take_by_name(&mut self, name: &str) -> TractResult<TVec<Tensor>> {
        let id = self.model().node_by_name(name)?.id;
        Self::take(self, id)
    }

    pub fn take(&mut self, id: usize) -> TractResult<TVec<Tensor>> {
        Ok(self.values[id]
            .take()
            .ok_or("Node is not computed")?
            .into_iter()
            .map(|v| Arc::try_unwrap(v).unwrap_or_else(|v| (*v).clone()))
            .collect())
    }

    pub fn plan(&self) -> &SimplePlan<TI, O, M> {
        &self.plans[0].borrow()
    }

    pub fn model(&self) -> &ModelImpl<TI, O> {
        self.plan().model()
    }
}
