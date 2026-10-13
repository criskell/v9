#[allow(non_camel_case_types)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CpuFeature {
    JSCVT,
    DOTPROD,
    LSE,
    PMULL1Q,
    FP16,
    SHA3,
    HBC,
    CSSC,
    MOPS,
    SVE,
    SVEBITPERM,
}

pub const NUMBER_OF_CPU_FEATURES: usize = CpuFeature::SVEBITPERM as usize + 1;

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub struct CpuFeatureSet {
    bits: u64,
}

impl CpuFeatureSet {
    pub fn new() -> CpuFeatureSet {
        CpuFeatureSet { bits: 0 }
    }

    pub fn contains(&self, feature: CpuFeature) -> bool {
        (self.bits & (1u64 << (feature as u32))) != 0
    }

    pub fn add(&mut self, feature: CpuFeature) {
        self.bits |= 1u64 << (feature as u32);
    }

    pub fn remove(&mut self, feature: CpuFeature) {
        self.bits &= !(1u64 << (feature as u32));
    }

    pub fn bits(&self) -> u64 {
        self.bits
    }
}
