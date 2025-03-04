use super::*;
use crate::model::TVec;

/// Infers every possible fact when all the values are concrete.
pub fn infer_forward_concrete(
    op: &dyn Op,
    inputs: &Vec<&TensorFact>,
) -> TractResult<Option<TVec<TensorFact>>> {
    let input_values: TVec<_> =
        inputs.iter().filter_map(|t| t.value.concretize()).map(|v| v.clone().into()).collect();

    if input_values.len() < inputs.len() {
        debug!("Can't infer value: some inputs are still unknown.");
        return Ok(None);
    }

    // If we know the value of all the inputs, we can deduce everything.
    if let Some(stateless) = op.as_stateless() {
        let output_value = stateless.eval(input_values)?.pop().unwrap();
        return Ok(Some(tvec![output_value.into()]));
    }

    Ok(None)
}

/// Infers basic shape facts in the case of broadcasting operators.
pub fn infer_shape_broadcasting(shapes: &[&ShapeFact]) -> TractResult<Option<ShapeFact>> {
    if shapes.iter().any(|s| s.is_open()) {
        debug!("Can't infer shape for broadcasting operators when some inputs have an open shape.");
        return Ok(None);
    }

    let bound = shapes.iter().map(|s| s.rank().concretize().unwrap()).max().unwrap() as usize;

    let mut output_shape: TVec<DimFact> = tvec![];

    for i in 0..bound {
        let mut previous: Option<TDim> = None;
        let mut unknown = 0;

        for shape in shapes.iter() {
            let rank = shape.rank().concretize().unwrap() as usize;
            let shape: TVec<DimFact> = shape.dims().collect();
            if i >= rank {
                continue;
            }

            match &shape[rank - i - 1] {
                GenericFact::Any => unknown += 1,
                GenericFact::Only(ref d) if d.is_one() => (),
                GenericFact::Only(ref d) => {
                    if previous.is_some() && previous.as_ref() != Some(d) {
                        bail!(
                            "Invalid shape (broadcasting): {:?} is not compatible with {:?}.",
                            d,
                            previous
                        )
                    } else {
                        previous = Some(d.clone())
                    }
                }
            };
        }

        if unknown > 1 {
            debug!("Can't infer shape (broadcasting): there are multiple unknown values at same index.");
            return Ok(None);
        } else if unknown == 1 && previous != None {
            debug!("Can't infer shape (broadcasting): there are both unknown and known values at same index.");
            return Ok(None);
        } else if unknown == 1 && previous == None {
            output_shape.push(GenericFact::Any);
        } else if let Some(previous) = previous {
            output_shape.push(GenericFact::Only(previous.clone()));
        } else {
            output_shape.push(GenericFact::Only(1.into()));
        }
    }

    output_shape.reverse();

    Ok(Some(ShapeFact::closed(output_shape)))
}

/// Infers basic facts in the case of unary or binary operators.
pub fn infer_forward_basic(
    op: &dyn Op,
    inputs: Vec<&TensorFact>,
) -> TractResult<Option<TVec<TensorFact>>> {
    if let Some(output) = infer_forward_concrete(op, &inputs)? {
        return Ok(Some(output));
    }

    // Otherwise we can only deduce the type and shape of the output.
    let input_shapes: Vec<_> = inputs.iter().map(|t| &t.shape).collect();

    let datum_type = inputs
        .iter()
        .filter_map(|i| i.datum_type.concretize())
        .next()
        .map(|t| typefact!(t))
        .unwrap_or(typefact!(_));

    let output = TensorFact {
        datum_type,
        shape: infer_shape_broadcasting(&input_shapes)?.unwrap_or(shapefact![..]),
        value: valuefact!(_),
    };

    Ok(Some(tvec![output]))
}

/// Returns the most specific closed shape out of an iterator.
pub fn most_specific_shape<'a, I: IntoIterator<Item = &'a ShapeFact>>(
    iter: I,
) -> TractResult<Option<&'a ShapeFact>> {
    let mut prev_rank = None;
    let mut prev_concrete = None;
    let mut best = None;

    for shape in iter {
        if let Some(rank) = shape.rank().concretize() {
            if prev_rank.is_some() && rank != prev_rank.unwrap() {
                bail!("Rank mismatch between different shapes.");
            } else {
                prev_rank = Some(rank);
            }

            let concrete = shape.dims().filter(|d| d.is_concrete()).count();

            if prev_concrete.is_none() || concrete > prev_concrete.unwrap() {
                prev_concrete = Some(concrete);
                best = Some(shape)
            }
        }
    }

    Ok(best)
}
