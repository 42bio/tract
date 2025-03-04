use tract_core::internal::*;

use crate::model::{ParsingContext, TfOpRegister};
use crate::tfpb::node_def::NodeDef;

pub fn register_all_ops(reg: &mut TfOpRegister) {
    reg.insert("Assign", |_, _| Ok(Box::new(Assign::default())));
    reg.insert("VariableV2", variable_v2);
}

fn variable_v2(_ctx: &ParsingContext, node: &NodeDef) -> TractResult<Box<dyn InferenceOp>> {
    let shared_name = node.get_attr_str("shared_name")?;
    let shared_name = if shared_name != "" { Some(shared_name) } else { None };
    let container = node.get_attr_str("container")?;
    let container = if container != "" { Some(container) } else { None };
    let name = node.get_name().to_string();
    let id = format!("{:?}#{:?}#{}", container, shared_name, name);
    let shape = node.get_attr_shape("shape")?;
    let dt = node.get_attr_datum_type("dtype")?;
    Ok(Box::new(VariableV2::new(container, shared_name, name, id, shape, dt)))
}

#[derive(Clone, Debug, new)]
struct VariableV2State;

impl OpState for VariableV2State {
    fn eval(
        &mut self,
        session: &mut SessionState,
        op: &dyn Op,
        _inputs: TVec<Arc<Tensor>>,
    ) -> TractResult<TVec<Arc<Tensor>>> {
        let op = op
            .downcast_ref::<VariableV2>()
            .ok_or_else(|| format!("wrong op for variable state"))?;
        let tensor = session
            .tensors
            .get(&op.id)
            .ok_or_else(|| format!("Could not find state for variable {}", op.id))?;
        Ok(tvec!(tensor.clone().into()))
    }
}

#[derive(Clone, Debug, new)]
pub struct VariableV2 {
    container: Option<String>,
    shared_name: Option<String>,
    name: String,
    pub id: String,
    shape: TVec<usize>,
    dt: DatumType,
}

impl Op for VariableV2 {
    fn name(&self) -> Cow<str> {
        "tf.VariableV2".into()
    }

    op_as_typed_op!();
}

impl StatefullOp for VariableV2 {
    fn state(
        &self,
        state: &mut SessionState,
        _node_id: usize,
    ) -> TractResult<Option<Box<dyn OpState>>> {
        fn make_buffer<T: Datum>(shape: &[usize]) -> Tensor {
            ::ndarray::ArrayD::<T>::default(shape).into()
        }

        let tensor = dispatch_datum!(make_buffer(self.dt)(&self.shape));
        state.tensors.insert(self.id.clone(), tensor);
        Ok(Some(Box::new(VariableV2State)))
    }
}

impl InferenceRulesOp for VariableV2 {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(inputs, 0)?;
        check_output_arity(outputs, 1)?;
        s.equals(&outputs[0].datum_type, self.dt)?;
        s.equals(&outputs[0].shape, ShapeFact::from(&*self.shape))?;
        Ok(())
    }

    inference_op_as_op!();
    to_typed!();
}

impl TypedOp for VariableV2 {
    typed_op_as_op!();

    fn output_facts(&self, _inputs: &[&TypedTensorInfo]) -> TractResult<TVec<TypedTensorInfo>> {
        Ok(tvec!(TypedTensorInfo::dt_shape(self.dt, &*self.shape)?))
    }
}

// need some dummy state to make sure Assign is a StatefullOp, and will not be
// eval-ed() in Stateless context
#[derive(Clone, Debug, new)]
struct AssignState;

#[derive(Clone, Debug, new, Default)]
pub struct Assign {
    pub var_id: Option<String>,
}

impl Op for Assign {
    fn name(&self) -> Cow<str> {
        "tf.Assign".into()
    }

    op_as_typed_op!();
}

impl OpState for AssignState {
    fn eval(
        &mut self,
        session: &mut SessionState,
        op: &dyn Op,
        mut inputs: TVec<Arc<Tensor>>,
    ) -> TractResult<TVec<Arc<Tensor>>> {
        let (_current, new) = args_2!(inputs);
        let op =
            op.downcast_ref::<Assign>().ok_or_else(|| format!("wrong op for variable state"))?;
        let var_id = if let Some(ref var_id) = op.var_id {
            var_id
        } else {
            bail!("Assign has not been linked to var")
        };
        fn assign<T: Datum>(
            session: &mut SessionState,
            var_id: &str,
            t: &Tensor,
        ) -> TractResult<()> {
            session
                .tensors
                .get_mut(var_id)
                .unwrap()
                .to_array_view_mut::<T>()?
                .assign(&t.to_array_view::<T>()?);
            Ok(())
        }
        dispatch_datum!(assign(new.datum_type())(session, var_id, &new))?;
        Ok(tvec!(new))
    }
}

impl StatefullOp for Assign {
    fn state(
        &self,
        _state: &mut SessionState,
        _node_id: usize,
    ) -> TractResult<Option<Box<dyn OpState>>> {
        Ok(Some(Box::new(AssignState)))
    }
}

impl InferenceRulesOp for Assign {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(inputs, 2)?;
        check_output_arity(outputs, 1)?;
        s.equals(&inputs[0].datum_type, &inputs[1].datum_type)?;
        s.equals(&outputs[0].datum_type, &inputs[0].datum_type)?;
        s.equals(&inputs[1].shape, &inputs[0].shape)?;
        s.equals(&outputs[0].shape, &inputs[0].shape)?;
        s.equals(&outputs[0].value, &inputs[1].value)?;
        Ok(())
    }

    inference_op_as_op!();
    to_typed!();
}

impl TypedOp for Assign {
    typed_op_as_op!();

    fn output_facts(&self, inputs: &[&TypedTensorInfo]) -> TractResult<TVec<TypedTensorInfo>> {
        Ok(tvec!(inputs[0].clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_assign() {
        let mut model = InferenceModel::default();

        let var = model
            .add_node_default(
                "var",
                VariableV2::new(None, None, "var".into(), "xxx".into(), tvec![], f32::datum_type()),
            )
            .unwrap();
        let zero = model.add_const("zero".to_string(), tensor0(0f32)).unwrap();
        let one = model.add_const("one".to_string(), tensor0(1f32)).unwrap();
        let reset = model.add_node_default("reset", Assign::new(Some("xxx".into()))).unwrap();
        model.add_edge(OutletId::new(var, 0), InletId::new(reset, 0)).unwrap();
        model.add_edge(OutletId::new(zero, 0), InletId::new(reset, 1)).unwrap();
        let set = model.add_node_default("set", Assign::new(Some("xxx".into()))).unwrap();
        model.add_edge(OutletId::new(var, 0), InletId::new(set, 0)).unwrap();
        model.add_edge(OutletId::new(one, 0), InletId::new(set, 1)).unwrap();
        model.auto_outputs().unwrap();
        let model = model.into_typed().unwrap();
        let model = std::rc::Rc::new(model);
        let var = model.node_id_by_name("var").unwrap();
        let plan_read = SimplePlan::new_for_output(model.clone(), OutletId::new(var, 0)).unwrap();
        let set = model.node_id_by_name("set").unwrap();
        let plan_set = SimplePlan::new_for_output(model.clone(), OutletId::new(set, 0)).unwrap();
        let reset = model.node_id_by_name("reset").unwrap();
        let plan_reset =
            SimplePlan::new_for_output(model.clone(), OutletId::new(reset, 0)).unwrap();
        let mut state = SimpleState::new_multiplan(vec![plan_read, plan_set, plan_reset]).unwrap();

        let read = state.run_plan(tvec!(), 0).unwrap(); // read
        assert_eq!(read, tvec!(Tensor::from(0.0f32).into()));
        let read = state.run_plan(tvec!(), 1).unwrap(); // set
        assert_eq!(read, tvec!(Tensor::from(1.0f32).into()));
        let read = state.run_plan(tvec!(), 0).unwrap(); // read
        assert_eq!(read, tvec!(Tensor::from(1.0f32).into()));
        let read = state.run_plan(tvec!(), 2).unwrap(); // reset
        assert_eq!(read, tvec!(Tensor::from(0.0f32).into()));
        let read = state.run_plan(tvec!(), 0).unwrap(); // read
        assert_eq!(read, tvec!(Tensor::from(0.0f32).into()));
    }
}
