#![allow(deprecated)]

use ndarray;
use tract_core;

error_chain! {
    types {
        CliError, CliErrorKind, CliResultExt, CliResult;
    }
    links {
        Tract(tract_core::TractError, tract_core::TractErrorKind);
        TractTensorflowConform(tract_tensorflow::conform::Error, tract_tensorflow::conform::ErrorKind) #[cfg(feature="conform")];
    }

    foreign_links {
        Io(::std::io::Error);
        NumParseInt(::std::num::ParseIntError);
        NdarrayShape(ndarray::ShapeError);
        NdarrayNpyReadNpz(ndarray_npy::ReadNpzError);
    }
}
