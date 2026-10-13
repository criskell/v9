#![allow(non_upper_case_globals)]

use crate::codegen::cpu_features::{CpuFeature, CpuFeatureSet};


pub struct AssemblerBuffer {
    buffer: Vec<u8>,
}

impl AssemblerBuffer {
    pub fn new() -> AssemblerBuffer {
        AssemblerBuffer { buffer: Vec::new() }
    }

    #[inline]
    pub fn start(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for AssemblerBuffer {
    fn default() -> AssemblerBuffer {
        AssemblerBuffer::new()
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct AssemblerOptions {
    pub enable_simulator_code: bool,
}

pub struct AssemblerBase {
    buffer: AssemblerBuffer,
    options: AssemblerOptions,
    enabled_cpu_features: CpuFeatureSet,
}

impl AssemblerBase {
    pub fn new() -> AssemblerBase {
        AssemblerBase {
            buffer: AssemblerBuffer::new(),
            options: AssemblerOptions::default(),
            enabled_cpu_features: CpuFeatureSet::new(),
        }
    }

    pub fn options(&self) -> &AssemblerOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: AssemblerOptions) {
        self.options = options;
    }

    pub fn is_enabled(&self, feature: CpuFeature) -> bool {
        self.enabled_cpu_features.contains(feature)
    }

    pub fn enable_cpu_feature(&mut self, feature: CpuFeature) {
        self.enabled_cpu_features.add(feature);
    }

    #[inline]
    pub fn pc_offset(&self) -> i32 {
        self.buffer.size() as i32
    }

    #[inline]
    pub fn buffer_start(&self) -> &[u8] {
        self.buffer.start()
    }

    #[inline]
    pub(crate) fn emit_bytes(&mut self, bytes: &[u8]) {
        self.buffer.buffer.extend_from_slice(bytes);
    }

    #[inline]
    pub(crate) fn write_bytes_at(&mut self, offset: i32, bytes: &[u8]) {
        let offset = offset as usize;
        self.buffer.buffer[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
}

impl Default for AssemblerBase {
    fn default() -> AssemblerBase {
        AssemblerBase::new()
    }
}
