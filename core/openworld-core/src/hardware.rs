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

/// This build does not load ONNX Runtime. The scan runs on the CPU.
pub fn loaded_execution() -> Execution {
    Execution::Cpu
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
    }
}
