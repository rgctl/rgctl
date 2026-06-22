//! Semantic translation and IDL generation

pub mod idl_generator;
pub mod signature;
pub mod type_inference;

pub use idl_generator::{IdlFormat, IdlGenerator};
pub use signature::{FunctionSignature, Param, SignatureExtractor};
pub use type_inference::{InferredType, TypeInference, TypeInferencer};
