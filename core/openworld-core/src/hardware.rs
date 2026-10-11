// SPDX-License-Identifier: Apache-2.0

use crate::copy::{CPU_NOTE, GPU_NOTE};
use serde::{Deserialize, Serialize};

/// What actually loaded. There is no reliable "has NPU?" bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    Cpu,
    Gpu,
    Neural,
}

impl Execution {
    pub fn as_str(self) -> &'static str {
        match self {
            Execution::Cpu => "cpu",
            Execution::Gpu => "gpu",
            Execution::Neural => "neural",
        }
    }

    /// CPU gets a before-Analyze note. GPU gets a shorter note. Neural gets none.
    pub fn device_note(self) -> Option<&'static str> {
        match self {
            Execution::Cpu => Some(CPU_NOTE),
            Execution::Gpu => Some(GPU_NOTE),
            Execution::Neural => None,
        }
    }
}

/// Map an ONNX Runtime provider name to the warning class.
///
/// NNAPI can land on a GPU or an NPU, so it takes the shorter GPU note.
/// QNN and CoreML / ANE are the neural path and get no warning.
pub fn execution_from_provider(provider: &str) -> Execution {
    let p = provider.to_ascii_lowercase();
    if p.contains("coreml") || p.contains("ane") || p.contains("qnn") || p.contains("neural") {
        Execution::Neural
    } else if p.contains("nnapi")
        || p.contains("cuda")
        || p.contains("directml")
        || p.contains("dml")
        || p.contains("vulkan")
        || p.contains("opengl")
        || p.contains("webgpu")
        || p.contains("gpu")
    {
        Execution::Gpu
    } else {
        Execution::Cpu
    }
}

/// The warning follows the provider that loaded.
/// A name on the command line is not that provider.
pub fn resolve_execution(_requested: &str) -> Execution {
    loaded_execution()
}

/// ONNX Runtime is linked. This build's prebuilt runtime loads the CPU provider.
/// Apple, Windows, and Android builds register CoreML, CUDA or DirectML, and NNAPI or QNN
/// when those providers are in the runtime that actually loads.
pub fn loaded_execution() -> Execution {
    crate::onnx_exec::active_execution()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_from_the_loaded_path() {
        assert_eq!(execution_from_provider("CPUExecutionProvider"), Execution::Cpu);
        assert_eq!(execution_from_provider("CUDAExecutionProvider"), Execution::Gpu);
        assert_eq!(execution_from_provider("DmlExecutionProvider"), Execution::Gpu);
        assert_eq!(execution_from_provider("NnapiExecutionProvider"), Execution::Gpu);
        assert_eq!(execution_from_provider("CoreMLExecutionProvider"), Execution::Neural);
        assert_eq!(execution_from_provider("QNNExecutionProvider"), Execution::Neural);
        assert!(Execution::Cpu.device_note().unwrap().contains("CPU"));
        assert!(Execution::Gpu.device_note().unwrap().contains("GPU"));
        assert!(Execution::Neural.device_note().is_none());
        assert_eq!(resolve_execution("CUDAExecutionProvider"), loaded_execution());
        assert_eq!(loaded_execution(), Execution::Cpu);
    }
}
