use vx::codegen::reloc_info::{Mode, RelocInfo, RelocInfoWriter, RelocIterator};

fn round_trip(entries: &[(i32, Mode, i64)]) -> Vec<(i32, Mode, i64)> {
    let mut writer = RelocInfoWriter::new();
    for (pc, mode, data) in entries {
        writer.write(&RelocInfo::new(*pc, *mode, *data));
    }
    let buffer = writer.buffer().to_vec();
    let mut read = Vec::new();
    let mut iterator = RelocIterator::new(&buffer, RelocIterator::kAllModesMask);
    while !iterator.done() {
        let rinfo = iterator.rinfo();
        read.push((rinfo.pc(), rinfo.rmode(), rinfo.data()));
        iterator.next();
    }
    read
}

#[test]
fn a_single_entry_round_trips() {
    let entries = [(8, Mode::CodeTarget, 0)];
    assert_eq!(round_trip(&entries), entries);
}

#[test]
fn the_short_tagged_modes_round_trip() {
    let entries = [
        (4, Mode::FullEmbeddedObject, 0),
        (8, Mode::CodeTarget, 0),
        (12, Mode::WasmStubCall, 0),
    ];
    assert_eq!(round_trip(&entries), entries);
}

#[test]
fn modes_with_int_data_round_trip() {
    let entries = [
        (4, Mode::ConstPool, 48),
        (64, Mode::VeneerPool, 12),
        (128, Mode::DeoptId, -3),
    ];
    assert_eq!(round_trip(&entries), entries);
}

#[test]
fn a_deopt_reason_uses_short_data() {
    let entries = [(16, Mode::DeoptReason, 42)];
    assert_eq!(round_trip(&entries), entries);
}

#[test]
fn a_long_pc_jump_is_encoded_and_decoded() {
    let entries = [
        (0, Mode::CodeTarget, 0),
        (1 << 20, Mode::CodeTarget, 0),
        ((1 << 20) + 4, Mode::ExternalReference, 0),
    ];
    assert_eq!(round_trip(&entries), entries);
}

#[test]
fn a_mask_filters_the_modes_the_iterator_reports() {
    let mut writer = RelocInfoWriter::new();
    for (pc, mode) in [
        (4, Mode::CodeTarget),
        (8, Mode::ExternalReference),
        (12, Mode::CodeTarget),
    ] {
        writer.write(&RelocInfo::new(pc, mode, 0));
    }
    let buffer = writer.buffer().to_vec();
    let mut seen = Vec::new();
    let mut iterator =
        RelocIterator::new(&buffer, RelocInfo::mode_mask(Mode::CodeTarget));
    while !iterator.done() {
        seen.push(iterator.rinfo().pc());
        iterator.next();
    }
    assert_eq!(seen, vec![4, 12]);
}

#[test]
fn the_mode_predicates_follow_the_order_of_the_enum() {
    assert!(RelocInfo::is_real_reloc_mode(Mode::CodeTarget));
    assert!(RelocInfo::is_real_reloc_mode(Mode::VeneerPool));
    assert!(!RelocInfo::is_real_reloc_mode(Mode::NoInfo));
    assert!(!RelocInfo::is_real_reloc_mode(Mode::DeoptId));
    assert!(RelocInfo::is_gc_reloc_mode(Mode::FullEmbeddedObject));
    assert!(!RelocInfo::is_gc_reloc_mode(Mode::ExternalReference));
    assert!(RelocInfo::is_shareable_reloc_mode(Mode::NoInfo));
    assert!(RelocInfo::is_shareable_reloc_mode(Mode::ExternalReference));
    assert!(!RelocInfo::is_shareable_reloc_mode(Mode::CodeTarget));
    assert!(RelocInfo::is_deopt_mode(Mode::DeoptReason));
    assert!(RelocInfo::is_embedded_object_mode(Mode::FullEmbeddedObject));
}
