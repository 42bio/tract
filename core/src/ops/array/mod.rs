/// # Operators on array and shapes
///
/// ## "Valid" reshaping operators
///
/// ### Both ONNX and TF
///
/// * Squeeze, unary (with or without an axis list, and... TF consider the empty list as
/// an absent list)
/// * Reshape, binary (input, shape as a tensor)
///
/// ### ONNX only
///
/// * Unsqueeze, unary, with required list of axes (referring to output)
/// * (Expand is a broadcasting operators, it does not beling here)
///
/// ### TF Only
///
/// * ExpandDims, binary (input, axis list)
///
/// ### Ours
///
/// * AddDims, just like ONNX's unsqueeze (Unsqueeze actually instantiate AddDims)
/// * RmDims, like Squeeze but with a mandatory axis list as an attribute.
///     Squeeze can always reduce to RmDims after inference.
///
/// ## Slicing and Upsampling
///
/// ### TF
///
/// * StridedSlice does everything
///
/// ### ONNX
///
/// * [Slice](https://github.com/onnx/onnx/blob/master/docs/Operators.md#Slice),
///     unary, attr are: begins, ends, and optional axes remapping them
/// * [Upsample](https://github.com/onnx/onnx/blob/master/docs/Operators.md#Upsample),
///     unary, attrs are scales (floats) and mode of interpolation (nearest or
///     linear). not impl.
/// * DynamicSlice, experimental, not impl
///
/// ### Ours
///
/// * Slice, unary, mandatory attrs are begin and end.
mod add_dims;
mod broadcast;
mod concat;
mod constant_like;
mod constant_of_shape;
mod crop;
mod flatten;
mod gather;
mod into_shape;
mod pad;
mod permute_axes;
mod reshape;
mod rm_dims;
mod shape;
mod size;
mod slice;
mod split;
mod squeeze;
mod tile;

pub use self::add_dims::AddDims;
pub use self::broadcast::{MultiBroadcastTo, TypedMultiBroadcastTo};
pub use self::concat::Concat;
pub use self::constant_like::ConstantLike;
pub use self::constant_like::EyeLike;
pub use self::constant_of_shape::ConstantOfShape;
pub use self::crop::Crop;
pub use self::flatten::Flatten;
pub use self::gather::Gather;
pub use self::into_shape::IntoShape;
pub use self::pad::{Pad, PadMode};
pub use self::permute_axes::PermuteAxes;
pub use self::reshape::Reshape;
pub use self::rm_dims::RmDims;
pub use self::shape::Shape;
pub use self::size::Size;
pub use self::slice::Slice;
pub use self::split::Split;
pub use self::squeeze::Squeeze;
pub use self::tile::Tile;
