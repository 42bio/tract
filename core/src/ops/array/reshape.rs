use crate::internal::*;

#[derive(Debug, Clone, new, Default)]
pub struct Reshape {}

impl Reshape {
    fn compute_shape<D: DimLike>(&self, input: &[D], shape: &[isize]) -> TractResult<Vec<D>> {
        if shape.iter().all(|d| *d > 0) {
            return Ok(shape.iter().map(|&d| D::from(d as usize)).collect());
        }
        let mut result: Vec<D> = shape
            .iter()
            .zip(input.iter().chain(std::iter::repeat(&D::from(1))))
            .map(|(&shape, input)| if shape > 0 { D::from(shape as usize) } else { input.clone() })
            .collect();
        if let Some(minus_one) = shape.iter().position(|d| *d == -1) {
            let prod_input: usize =
                input.iter().try_fold(1, |acc, dim| dim.to_integer().map(|a| a as usize * acc))?;
            let prod_shape: usize = result
                .iter()
                .enumerate()
                .filter(|(ix, _)| *ix != minus_one)
                .try_fold(1, |acc, (_, dim)| dim.to_integer().map(|a| a as usize * acc))?;
            result[minus_one] = D::from(prod_input / prod_shape);
        }
        Ok(result)
    }

    /// Evaluates the operation given the input tensors.
    fn eval_t<T: Datum>(
        &self,
        input: Arc<Tensor>,
        shape: &[usize],
    ) -> TractResult<TVec<Arc<Tensor>>> {
        Ok(tvec![input.into_tensor().into_array::<T>()?.into_shape(shape)?.into_arc_tensor()])
    }
}

impl Op for Reshape {
    fn name(&self) -> Cow<str> {
        "Reshape".into()
    }

    not_a_typed_op!();
}

impl StatelessOp for Reshape {
    fn eval(&self, mut inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        let (input, shape) = args_2!(inputs);
        let shape: Vec<isize> =
            shape.cast_to::<i64>()?.to_array_view::<i64>()?.iter().map(|&i| i as isize).collect();
        let oshape = self.compute_shape(input.shape(), &shape)?;
        dispatch_datum!(Self::eval_t(input.datum_type())(self, input, &oshape))
    }
}

impl InferenceRulesOp for Reshape {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        s.equals(&outputs[0].datum_type, &inputs[0].datum_type)?;
        s.given_2(&inputs[0].shape, &inputs[1].value, move |s, ishape, shape| {
            let shape: Vec<isize> = shape
                .cast_to::<i64>()?
                .to_array_view::<i64>()?
                .iter()
                .map(|&i| i as isize)
                .collect();
            let shape = self.compute_shape(&ishape, &shape)?;
            s.equals(&outputs[0].shape, ShapeFact::from(shape))
        })
    }

    fn to_typed(
        &self,
        _source: &InferenceModel,
        node: &InferenceNode,
        target: &mut TypedModel,
        mapping: &HashMap<OutletId, OutletId>,
    ) -> TractResult<TVec<OutletId>> {
        if let Some(ref shape) = target.outlet_fact(mapping[&node.inputs[1]])?.konst {
            let shape: TVec<usize> =
                shape.cast_to::<i64>()?.as_slice::<i64>()?.iter().map(|i| *i as usize).collect();
            let op = super::IntoShape::new(shape);
            return target.wire_node(&*node.name, op, [mapping[&node.inputs[0]]].as_ref());
        }
        bail!("shape input is variable")
    }

    inference_op_as_op!();
}
