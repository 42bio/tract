use ndarray::prelude::*;
use num_traits::AsPrimitive;

use crate::internal::*;

#[derive(Debug, Clone, new)]
pub struct Shape {
    dt: DatumType,
}

impl Shape {
    pub fn coerce_to<T>(shape: &[usize]) -> TractResult<Arc<Tensor>>
    where
        T: Copy + Datum,
        usize: AsPrimitive<T>,
    {
        let array = Array1::from_vec(shape.iter().map(|i| i.as_()).collect());
        Ok(array.into_arc_tensor())
    }
}

impl Op for Shape {
    fn name(&self) -> Cow<str> {
        "Shape".into()
    }

    op_as_typed_op!();
}

impl StatelessOp for Shape {
    /// Evaluates the operation given the input tensors.
    fn eval(&self, inputs: TVec<Arc<Tensor>>) -> TractResult<TVec<Arc<Tensor>>> {
        let shape = inputs[0].shape();
        Ok(tvec![dispatch_numbers!(Self::coerce_to(self.dt)(&shape))?])
    }
}

impl InferenceRulesOp for Shape {
    fn rules<'r, 'p: 'r, 's: 'r>(
        &'s self,
        s: &mut Solver<'r>,
        inputs: &'p [TensorProxy],
        outputs: &'p [TensorProxy],
    ) -> InferenceResult {
        check_input_arity(&inputs, 1)?;
        check_output_arity(&outputs, 1)?;
        s.equals(&outputs[0].rank, 1)?;
        s.given(&inputs[0].rank, move |s, r| s.equals(&outputs[0].shape[0], r.to_dim()))?;
        s.given(&outputs[0].shape[0], move |s, r| {
            if let Ok(d) = r.to_integer() {
                s.equals(&inputs[0].rank, d)?;
            }
            Ok(())
        })?;
        s.given(&inputs[0].shape, move |s, shape| {
            if shape.iter().any(|d| d.to_integer().is_err()) {
                s.equals(&outputs[0].datum_type, DatumType::TDim)?;
                let array1: Array1<TDim> = Array1::from_iter(shape);
                let tensor = array1.into_arc_tensor();
                s.equals(&outputs[0].value, tensor)
            } else if self.dt == DatumType::I64 {
                s.equals(&outputs[0].datum_type, DatumType::I64)?;
                let array1: Array1<i64> = Array1::from_vec(
                    shape.iter().map(|i| i.to_integer().unwrap() as i64).collect(),
                );
                let tensor = array1.into_arc_tensor();
                s.equals(&outputs[0].value, tensor)
            } else {
                s.equals(&outputs[0].datum_type, DatumType::I32)?;
                let array1: Array1<i32> = Array1::from_vec(
                    shape.iter().map(|i| i.to_integer().unwrap() as i32).collect(),
                );
                let tensor = array1.into_arc_tensor();
                s.equals(&outputs[0].value, tensor)
            }
        })
    }

    inference_op_as_op!();
    to_typed!();
}

impl TypedOp for Shape {
    typed_op_as_op!();

    fn output_facts(&self, inputs: &[&TypedTensorInfo]) -> TractResult<TVec<TypedTensorInfo>> {
        let shape = inputs[0].shape.iter().collect::<TVec<_>>();
        let mut tensor = tensor1(&*shape);
        if shape.iter().all(|d| d.to_integer().is_ok()) {
            tensor = tensor.cast_to_dt(self.dt)?.into_owned();
        }
        Ok(tvec!(TypedTensorInfo::from(tensor)))
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_inference_1() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, _, _; ..],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![_],
            value: valuefact!(_),
        };

        assert_forward!(Shape::new(DatumType::I32), input, output);
    }

    #[test]
    fn shape_inference_2() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, _, _],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(_),
        };

        assert_forward!(Shape::new(DatumType::I32), input, output);
    }

    #[test]
    fn shape_inference_3() {
        let input = TensorFact {
            datum_type: typefact!(DatumType::F32),
            shape: shapefact![1, 2, 3],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(Tensor::dims(&[3], &[1.to_dim(), 2.to_dim(), 3.to_dim()]).unwrap()),
        };

        assert_forward!(Shape::new(DatumType::I32), input, output);
    }

    #[test]
    fn shape_inference_4() {
        let input = TensorFact {
            datum_type: typefact!(_),
            shape: shapefact![1, 2, 3],
            value: valuefact!(_),
        };

        let output = TensorFact {
            datum_type: typefact!(DatumType::TDim),
            shape: shapefact![3],
            value: valuefact!(Tensor::dims(&[3], &[1.to_dim(), 2.to_dim(), 3.to_dim()]).unwrap()),
        };

        assert_backward!(Shape::new(DatumType::I32), input, output);
    }
}
*/
