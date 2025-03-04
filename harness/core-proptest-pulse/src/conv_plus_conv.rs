use proptest::proptest;
use proptest::test_runner::TestCaseResult;
use tract_core::dimfact;
use tract_core::internal::*;
use tract_core::ndarray::*;
use tract_core::shapefact;

use super::*;

#[derive(Debug, Clone)]
struct ConvOp {
    stride: usize,
    dilation: usize,
    ker: Array3<f32>,
}
impl ConvOp {
    fn chain(&self, name: &str, model: &mut InferenceModel, after: OutletId) -> OutletId {
        let mut conv = tract_core::ops::cnn::Conv::default();
        conv.dilations = Some(tvec!(self.dilation));
        conv.strides = Some(tvec!(self.stride));
        let conv = model.chain_after(after, name, conv, tvec!(TensorFact::default())).unwrap();
        model
            .plug_const(InletId::new(conv, 1), format!("{}-kernel", name), self.ker.clone())
            .unwrap();
        OutletId::new(conv, 0)
    }
}

impl Arbitrary for ConvOp {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> BoxedStrategy<Self> {
        (1usize..3, 1usize..3, vec(1usize..3))
            .prop_map(|(stride, dilation, ker)| ConvOp {
                stride,
                dilation,
                ker: Array3::from_shape_vec((1, 1, ker.len()), ker).unwrap(),
            })
            .boxed()
    }
}

#[derive(Debug, Clone)]
struct ConvPlusConvProblem {
    input: Array3<f32>,
    pulse: usize,
    conv1: ConvOp,
    conv2: ConvOp,
}

impl Arbitrary for ConvPlusConvProblem {
    type Parameters = ();
    type Strategy = BoxedStrategy<Self>;

    fn arbitrary_with(_: Self::Parameters) -> BoxedStrategy<Self> {
        (ConvOp::arbitrary(), ConvOp::arbitrary(), 1usize..3)
            .prop_flat_map(|(conv1, conv2, pulse_factor)| {
                let pulse = conv1.stride * conv2.stride * pulse_factor;
                let min_input = 10usize;
                (Just(conv1), Just(conv2), Just(pulse), vec(min_input..3 * min_input))
            })
            .prop_map(|(conv1, conv2, pulse, input)| {
                let input = Array3::from_shape_vec((1, 1, input.len()), input).unwrap(); // NCHW
                ConvPlusConvProblem { input, pulse, conv1, conv2 }
            })
            .boxed()
    }
}

impl ConvPlusConvProblem {
    pub fn run(&self) -> TestCaseResult {
        let mut model = InferenceModel::default();
        let input = model
            .add_source("a", TensorFact::dt_shape(f32::datum_type(), shapefact!(1, 1, S)))
            .unwrap();
        let id = self.conv1.chain("conv1", &mut model, OutletId::new(input, 0));
        let _id = self.conv2.chain("conv2", &mut model, id);
        model.auto_outputs().unwrap();
        proptest_regular_against_pulse(model, self.pulse as _, self.input.clone().into_dyn(), 2)
    }
}

proptest! {
    #[test]
    fn proptest(pb in ConvPlusConvProblem::arbitrary()) { pb.run().unwrap() }
}
