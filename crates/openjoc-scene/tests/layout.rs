use openjoc_oamd::{BedAssignment, ContentDescription, OamdContentPrefix};
use openjoc_scene::{
    ObjectAudioSource, ProgrammeLayout, ProgrammeLayoutError, ProgrammeObjectClass,
};

fn prefix(object_count: u16, content: ContentDescription) -> OamdContentPrefix {
    OamdContentPrefix {
        syntax_version: 0,
        object_count,
        content,
        alternate_object_data_present: false,
        element_count: 1,
        consumed_bits: 0,
    }
}

#[test]
fn optional_lfe_maps_outside_fifteen_dynamic_joc_rows() {
    let layout = ProgrammeLayout::from_prefix(&prefix(
        16,
        ContentDescription::DynamicOnly { lfe_present: true },
    ))
    .expect("dynamic-only LFE layout");
    assert_eq!(layout.total_oamd_count, 16);
    assert_eq!(layout.speaker_anchored_count, 1);
    assert_eq!(layout.bed_count, 0);
    assert_eq!(layout.lfe_count, 1);
    assert_eq!(layout.isf_count, 0);
    assert_eq!(layout.dynamic_slot_count, 15);
    assert_eq!(
        layout.bindings[0].source,
        ObjectAudioSource::BaseLfe { channel_index: 0 }
    );
    assert_eq!(layout.bindings[0].class, ProgrammeObjectClass::Lfe);
    assert_eq!(
        layout.bindings[1].source,
        ObjectAudioSource::JocObject { row_index: 0 }
    );
    assert_eq!(
        layout.bindings[15].source,
        ObjectAudioSource::JocObject { row_index: 14 }
    );
    layout.validate_joc_output(15).expect("fifteen JOC rows");
    let joc = (0..15).map(|row| vec![row as f64; 4]).collect::<Vec<_>>();
    let lfe = vec![99.0; 4];
    let scene_order = layout
        .bind_audio(&joc, Some(&lfe))
        .expect("bound programme");
    assert_eq!(scene_order.len(), 16);
    assert_eq!(scene_order[0], lfe);
    assert_eq!(scene_order[1], joc[0]);
    assert_eq!(scene_order[15], joc[14]);
}

#[test]
fn dynamic_only_without_lfe_is_one_to_one() {
    let layout = ProgrammeLayout::from_prefix(&prefix(
        2,
        ContentDescription::DynamicOnly { lfe_present: false },
    ))
    .expect("dynamic-only layout");
    assert_eq!(layout.dynamic_slot_count, 2);
    assert_eq!(layout.speaker_anchored_count, 0);
    assert_eq!(
        layout.bindings[0].source,
        ObjectAudioSource::JocObject { row_index: 0 }
    );
    assert_eq!(
        layout.bindings[1].source,
        ObjectAudioSource::JocObject { row_index: 1 }
    );
    layout.validate_joc_output(2).expect("two JOC rows");
    assert_eq!(
        layout
            .bind_audio(&[vec![1.0; 4], vec![2.0; 4]], None)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn layout_rejects_unsupported_bed_and_lfe_source_failures() {
    let bed = ProgrammeLayout::from_prefix(&prefix(
        2,
        ContentDescription::Mixed {
            bed_channel_distribute: Some(false),
            beds: vec![BedAssignment::LfeOnly],
            intermediate_spatial_format: None,
            dynamic_objects: Some(1),
        },
    ))
    .expect("bed layout");
    assert_eq!(bed.lfe_count, 1);
    bed.validate_joc_output(1)
        .expect("LFE-only bed is supported");
    assert_eq!(
        bed.bind_audio(&[vec![1.0; 4]], None),
        Err(ProgrammeLayoutError::BaseLfeUnavailable)
    );

    let ordinary_bed = ProgrammeLayout::from_prefix(&prefix(
        2,
        ContentDescription::Mixed {
            bed_channel_distribute: Some(false),
            beds: vec![BedAssignment::Standard(1 << 8)],
            intermediate_spatial_format: None,
            dynamic_objects: Some(1),
        },
    ))
    .expect("ordinary bed layout");
    assert_eq!(
        ordinary_bed.validate_joc_output(1),
        Err(ProgrammeLayoutError::UnsupportedBedToJocMapping { oamd_index: 0 })
    );

    let multiple_lfe = ProgrammeLayout::from_prefix(&prefix(
        2,
        ContentDescription::Mixed {
            bed_channel_distribute: Some(false),
            beds: vec![BedAssignment::LfeOnly, BedAssignment::LfeOnly],
            intermediate_spatial_format: None,
            dynamic_objects: None,
        },
    ))
    .expect("multiple LFE layout");
    assert_eq!(
        multiple_lfe.validate_joc_output(0),
        Err(ProgrammeLayoutError::MultipleLfeObjects { count: 2 })
    );
}

#[test]
fn dynamic_slot_count_mismatch_is_not_a_generic_object_count_match() {
    let layout = ProgrammeLayout::from_prefix(&prefix(
        3,
        ContentDescription::DynamicOnly { lfe_present: true },
    ))
    .expect("three total entries");
    assert_eq!(layout.total_oamd_count, 3);
    assert_eq!(layout.dynamic_slot_count, 2);
    assert_eq!(
        layout.validate_joc_output(1),
        Err(ProgrammeLayoutError::JocDynamicCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
}
