#![cfg(feature = "conform")]
#![allow(non_snake_case)]
extern crate env_logger;
#[macro_use]
extern crate log;
extern crate ndarray;
#[macro_use]
extern crate proptest;
extern crate protobuf;
extern crate tract_core;
extern crate tract_tensorflow;

mod utils;

use crate::utils::*;
use ndarray::prelude::*;
use proptest::prelude::*;
use protobuf::Message;
use tract_core::prelude::*;
use tract_tensorflow::conform::*;
use tract_tensorflow::tfpb;
use tract_tensorflow::tfpb::types::DataType::DT_FLOAT;

fn convolution_pb(stride: usize, valid: bool, k: &Tensor) -> Result<Vec<u8>> {
    let conv = tfpb::node()
        .name("conv")
        .op("DepthwiseConv2dNative")
        .input("data")
        .input("kernel")
        .attr("strides", vec![1, stride as i64, stride as i64, 1])
        .attr("dilations", vec![1, 1, 1, 1])
        .attr("padding", if valid { "VALID" } else { "SAME" })
        .attr("T", DT_FLOAT);

    let graph = tfpb::graph().node(placeholder_f32("data")).node(const_f32("kernel", k)).node(conv);

    Ok(graph.write_to_bytes()?)
}

fn img_and_ker() -> BoxedStrategy<(Array4<f32>, Array4<f32>, usize)> {
    (1usize..3, 1usize..3, 1usize..3, 1usize..3, 1usize..3)
        .prop_flat_map(|(ic, kh, kw, q, s)| {
            (
                1usize..3,
                (kh + s..2 * kh + 4 * s),
                (kw + s..2 * kw + 4 * s),
                Just((ic, kh, kw, q, s)),
            )
        })
        .prop_flat_map(|(ib, ih, iw, (ic, kh, kw, q, s))| {
            let i_size = ib * iw * ih * ic;
            let k_size = kw * kh * ic * q;
            (
                Just((ib, ih, iw, ic)),
                Just((kh, kw, ic, q)),
                ::proptest::collection::vec(-9i32..9, i_size..i_size + 1),
                ::proptest::collection::vec(-9i32..9, k_size..k_size + 1),
                Just(s),
            )
        })
        .prop_map(|(img_shape, ker_shape, img, ker, stride)| {
            (
                Array::from_vec(img.into_iter().map(|i| i as f32).collect())
                    .into_shape(img_shape)
                    .unwrap(),
                Array::from_vec(ker.into_iter().map(|i| i as f32).collect())
                    .into_shape(ker_shape)
                    .unwrap(),
                stride,
            )
        })
        .boxed()
}

proptest! {
    #[test]
    fn conv_compare((ref i, ref k, stride) in img_and_ker(),
                       valid in ::proptest::bool::ANY) {
        let k = Tensor::from(k.clone());
        let model = convolution_pb(stride, valid, &k).unwrap();
        compare(&model, vec!(("data", i.clone().into()), ), "conv")?;
    }
}

proptest! {
    #[test]
    fn conv_infer_facts((ref i, ref k, stride) in img_and_ker(),
                       valid in ::proptest::bool::ANY) {
        let k = Tensor::from(k.clone());
        let model = convolution_pb(stride, valid, &k).unwrap();
        infer(&model, vec!(("data", i.clone().into())), "conv")?;
    }
}

#[test]
fn conv_infer_facts_1() {
    let i: Tensor = ArrayD::<f32>::zeros(vec![1, 2, 2, 2]).into();
    let k: Tensor = ArrayD::<f32>::zeros(vec![2, 2, 2, 1]).into();
    let model = convolution_pb(1, false, &k).unwrap();
    infer(&model, vec![("data", i.clone().into())], "conv").unwrap();
}

#[test]
fn conv_eval_1() {
    let i: Tensor = Tensor::from(arr4(&[[[[0.0f32, 0.0], [1.0, 0.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[0.0f32], [0.0]], [[1.0], [0.0]]]]));
    let model = convolution_pb(1, false, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_2() {
    let i: Tensor = Tensor::from(arr4::<f32, _, _, _>(&[[[[-1.0], [0.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[1.0f32]]]]));
    let model = convolution_pb(2, false, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_3() {
    use rand::distributions::Uniform;

    let i: Tensor =
        Tensor::from(Array::from_shape_fn((1, 112, 112, 48), |_| rand::random::<f32>()));
    let k: Tensor = Tensor::from(Array::from_shape_fn((3, 3, 48, 1), |_| rand::random::<f32>()));
    let conv = tfpb::node()
        .name("conv")
        .op("DepthwiseConv2dNative")
        .input("data")
        .input("kernel")
        .attr("strides", vec![1, 1, 1, 1])
        .attr("dilations", vec![1, 1, 1, 1])
        .attr("padding", "SAME")
        .attr("T", DT_FLOAT);

    let graph =
        tfpb::graph().node(placeholder_f32("data")).node(const_f32("kernel", &k)).node(conv);

    let model = graph.write_to_bytes().unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_4() {
    let i: Tensor = Tensor::from(arr4(&[[[[0.0f32], [0.0]], [[0.0], [-1.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[0.0f32, -1.0]]]]));
    let model = convolution_pb(1, false, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_5() {
    let i: Tensor = Tensor::from(arr4(&[[[[0.0f32, 0.0], [0.0, 0.0]], [[0.0, 0.0], [0.0, 1.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[0.0f32, 0.0], [1.0, 0.0]]]]));
    let model = convolution_pb(1, false, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_6() {
    let i: Tensor = Tensor::from(arr4(&[[[[0.0f32, 0.0], [0.0, 0.0]], [[0.0, 0.0], [0.0, 1.0]]]]));
    let k: Tensor = Tensor::from(arr4(&[[[[0.0f32], [1.0]]]]));
    let model = convolution_pb(1, true, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}

#[test]
fn conv_eval_7() {
    let i: Tensor = tensor4(&[[[[1.0f32, 2.0]]]]);
    let k: Tensor = tensor4(&[[[[3.0f32, 5.0], [7.0, 11.0]]]]);
    let model = convolution_pb(1, false, &k).unwrap();
    compare(&model, vec![("data", i.into())], "conv").unwrap();
}
