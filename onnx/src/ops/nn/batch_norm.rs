use ndarray::Axis;
use num_traits::AsPrimitive;
use tract_core::internal::*;
use tract_core::ops::nn::DataFormat;

#[derive(Debug, Clone, new, Default)]
pub struct BatchNorm {
    data_format: DataFormat,
    epsilon: f32,
    spatial: bool,
}

impl BatchNorm {
    fn to_slope_and_inter<T>(
        &self,
        c_dim: usize,
        scale: &Tensor,
        beta: &Tensor,
        mean: &Tensor,
        var: &Tensor,
    ) -> TractResult<(Tensor, Tensor)>
    where
        T: Datum + ::num_traits::Float + ::num_traits::FromPrimitive + ::ndarray::ScalarOperand,
        f32: AsPrimitive<T>,
    {
        let scale = scale.to_array_view::<T>()?.into_shape((c_dim,))?;
        let beta = beta.to_array_view::<T>()?.into_shape((c_dim,))?;
        let mean = mean.to_array_view::<T>()?.into_shape((c_dim,))?;
        let var = var.to_array_view::<T>()?.into_shape((c_dim,))?;

        let denominator = (var.to_owned() + self.epsilon.as_()).map(|x| x.sqrt());

        let slope = &scale / &denominator;
        let intercept = beta.to_owned() - (&mean * &scale) / denominator;
        Ok((slope.into_tensor(), intercept.into_tensor()))
    }

    fn eval_t<T>(&self, mut inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>>
    where
        T: Datum + ::num_traits::Float + ::num_traits::FromPrimitive + ::ndarray::ScalarOperand,
        f32: AsPrimitive<T>,
    {
        let (x, scale, beta, mean, var) = args_5!(&mut inputs);

        let c_axis = self.data_format.shape(x.shape()).c_axis();
        let c_dim = *self.data_format.shape(x.shape()).c_dim();

        let (slope, intercept) = self.to_slope_and_inter::<T>(c_dim, &scale, &beta, &mean, &var)?;

        let slope = slope.as_slice::<T>()?;
        let intercept = intercept.as_slice::<T>()?;
        let mut x = x.into_tensor().into_array::<T>()?;

        for c in 0..c_dim {
            x.slice_axis_mut(Axis(c_axis), (c..=c).into())
                .mapv_inplace(|x| x * slope[c] + intercept[c]);
        }
        return Ok(tvec!(x.into_arc_tensor()));
    }
}

impl Op for BatchNorm {
    fn name(&self) -> Cow<str> {
        "BatchNorm".into()
    }

    not_a_typed_op!();
}

impl StatelessOp for BatchNorm {
    fn eval(&self, inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        dispatch_floatlike!(Self::eval_t(inputs[0].datum_type())(self, inputs))
    }
}

impl InferenceRulesOp for BatchNorm {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(&inputs, 5)?;
        check_output_arity(&outputs, 1)?;
        s.equals_all(wrap!(
            &outputs[0].datum_type,
            &inputs[0].datum_type,
            &inputs[1].datum_type,
            &inputs[2].datum_type,
            &inputs[3].datum_type,
            &inputs[4].datum_type
        ))?;
        s.equals(&inputs[0].shape, &outputs[0].shape)?;
        s.equals_all(wrap!(
            &inputs[1].shape,
            &inputs[2].shape,
            &inputs[3].shape,
            &inputs[4].shape
        ))?;
        s.given(&inputs[0].shape, move |s, shape| {
            let shape = self.data_format.shape(shape);
            s.equals(&inputs[1].shape[0], shape.c_dim())
        })?;
        Ok(())
    }

    inference_op_as_op!();

    fn to_typed(
        &self,
        _source: &InferenceModel,
        node: &InferenceNode,
        target: &mut TypedModel,
        mapping: &HashMap<OutletId, OutletId>,
    ) -> TractResult<TVec<OutletId>> {
        let x = target.outlet_fact(mapping[&node.inputs[0]])?;
        dbg!(&x);
        let params = (1..5)
            .map(|i| Ok(target.outlet_fact(mapping[&node.inputs[i]])?.konst.clone()))
            .collect::<TractResult<TVec<Option<Arc<Tensor>>>>>()?;

        if let (Some(scale), Some(beta), Some(mean), Some(var)) =
            (params[0].as_ref(), params[1].as_ref(), params[2].as_ref(), params[3].as_ref())
        {
            let x_shape = x.shape.to_tvec();
            let c_axis = self.data_format.shape(&x_shape).c_axis();
            let c_dim = self.data_format.shape(&x_shape).c_dim().to_integer()? as usize;

            let (slope, inter) = dispatch_floatlike!(Self::to_slope_and_inter(x.datum_type)(
                self, c_dim, &scale, &beta, &mean, &var
            ))?;

            let mut param_shape: TVec<usize> = slope.shape().into();
            if c_axis < x_shape.len() - 1 {
                for _i in c_axis..x_shape.len() - 1 {
                    param_shape.push(1);
                }
            }
            let slope = unsafe { slope.into_shape(&param_shape)? };
            let inter = unsafe { inter.into_shape(&param_shape)? };

            let wire = mapping[&node.inputs[0]];
            let wire = target.wire_node(
                format!("{}-mul", node.name),
                tract_core::ops::math::mul::unary(slope.into_arc_tensor()),
                [wire].as_ref(),
            )?;
            return target.wire_node(
                &*node.name,
                tract_core::ops::math::add::unary(inter.into_arc_tensor()),
                &wire,
            );
        }
        bail!("Params are not const")
    }
}
