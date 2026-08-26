use openjoc_oamd::{ContentDescription, OamdContentPrefix, OamdPayload};
use openjoc_scene::{
    BindingCodecProfile, BoundDecodedJocObject, DecodedJocBindingFacts, Extent3,
    JocDecodedObjectOrdinal, MetadataUpdate, OamdBindingObjectClass, Position, Position3,
    ZoneConstraint, admit_decoded_joc_binding,
};

fn admitted_facts() -> DecodedJocBindingFacts {
    let mut oamd_total_classes = vec![OamdBindingObjectClass::BaseLfe];
    oamd_total_classes.extend(std::iter::repeat_n(OamdBindingObjectClass::Dynamic, 15));
    DecodedJocBindingFacts::new(
        BindingCodecProfile::EAc3JocObservedOrdinary,
        15,
        15,
        oamd_total_classes,
    )
}

#[test]
fn admitted_profile_maps_lower_middle_and_upper_ordinals_without_audio_inputs() {
    let profile = admit_decoded_joc_binding(&admitted_facts()).expect("admitted profile");
    let bound = profile.bind_decoded_objects().expect("bound objects");

    for (joc, dynamic, total) in [(0, 0, 1), (3, 3, 4), (14, 14, 15)] {
        let object = &bound[joc];
        assert_eq!(object.joc_ordinal.0, joc);
        assert_eq!(object.oamd_dynamic_ordinal.0, dynamic);
        assert_eq!(object.oamd_total_index.0, total);
    }
}

#[test]
fn scoped_warp3_compat_profile_admits_only_the_same_ordinary_structure() {
    let mut facts = admitted_facts();
    facts.codec_profile = BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3;

    let profile = admit_decoded_joc_binding(&facts).expect("scoped compat profile");
    assert_eq!(
        profile.codec_profile(),
        BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3
    );

    facts.replace_total_class(1, OamdBindingObjectClass::Bed);
    assert!(admit_decoded_joc_binding(&facts).is_err());
}

#[test]
fn admitted_binding_preserves_slots_and_uses_typed_row_domain() {
    let profile = admit_decoded_joc_binding(&admitted_facts()).expect("admitted profile");
    let bound = profile.bind_decoded_objects().expect("bound objects");

    assert_eq!(bound.len(), 15);
    assert_eq!(bound[7].reconstruction_row.0, 7);
    assert_eq!(
        bound[7],
        BoundDecodedJocObject {
            joc_ordinal: openjoc_scene::JocDecodedObjectOrdinal(7),
            reconstruction_row: openjoc_joc::ReconstructionBasisRowIndex(7),
            oamd_dynamic_ordinal: openjoc_scene::OamdDynamicObjectOrdinal(7),
            oamd_total_index: openjoc_scene::OamdTotalObjectIndex(8),
        }
    );
}

#[test]
fn typed_bridge_conversions_keep_each_index_domain_explicit() {
    let row = openjoc_joc::ReconstructionBasisRowIndex(7);
    let joc = JocDecodedObjectOrdinal::from_reconstruction_row(row);

    assert_eq!(joc.reconstruction_row(), row);
    assert_eq!(joc.oamd_dynamic_ordinal().0, 7);
    assert_eq!(joc.oamd_total_index().expect("total index").0, 8);
    assert_eq!(
        JocDecodedObjectOrdinal(usize::MAX).oamd_total_index(),
        Err(openjoc_scene::DecodedJocBindingUnavailable::OrdinalOverflow)
    );
}

#[test]
fn bound_scene_views_borrow_row_pcm_and_only_the_matching_oamd_events() {
    let profile = admit_decoded_joc_binding(&admitted_facts()).expect("admitted profile");
    let basis = openjoc_joc::ReconstructionBasis {
        rows: (0..15).map(|index| vec![index as f64]).collect(),
    };
    let metadata = (1..=15)
        .map(|object_id| MetadataUpdate {
            object_id,
            start_sample: 0,
            ramp_duration: 0,
            active: true,
            position: Position::Room(Position3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }),
            size: Extent3 {
                width: 0.0,
                depth: 0.0,
                height: 0.0,
            },
            priority: 0.0,
            gain_db: None,
            channel_lock: false,
            zones: [ZoneConstraint::Include; 6],
            divergence: 0.0,
            trim_disabled: false,
        })
        .collect::<Vec<_>>();

    let bound = profile
        .bind_scene_objects(&basis, &metadata)
        .expect("bound decoded scene");

    assert_eq!(bound.len(), 15);
    assert_eq!(bound[7].pcm, &[7.0]);
    assert_eq!(bound[7].metadata.len(), 1);
    assert_eq!(bound[7].metadata[0].object_id, 8);
    assert_eq!(
        bound[7].binding_profile,
        BindingCodecProfile::EAc3JocObservedOrdinary
    );
}

#[test]
fn bed_profile_is_not_admitted() {
    let mut facts = admitted_facts();
    facts.replace_total_class(1, OamdBindingObjectClass::Bed);

    assert!(matches!(
        admit_decoded_joc_binding(&facts),
        Err(openjoc_scene::DecodedJocBindingUnavailable::UnsupportedBed { .. })
    ));
}

#[test]
fn isf_profile_is_not_admitted() {
    let mut facts = admitted_facts();
    facts.replace_total_class(1, OamdBindingObjectClass::Isf);

    assert!(matches!(
        admit_decoded_joc_binding(&facts),
        Err(openjoc_scene::DecodedJocBindingUnavailable::UnsupportedIsf { .. })
    ));
}

#[test]
fn alternative_lfe_layout_is_not_admitted() {
    let mut facts = admitted_facts();
    facts.replace_total_class(0, OamdBindingObjectClass::Dynamic);

    assert!(matches!(
        admit_decoded_joc_binding(&facts),
        Err(openjoc_scene::DecodedJocBindingUnavailable::BaseLfeBindingPrecondition)
    ));
}

#[test]
fn count_mismatch_is_not_admitted() {
    let mut facts = admitted_facts();
    facts.set_joc_object_count(14);

    assert!(matches!(
        admit_decoded_joc_binding(&facts),
        Err(openjoc_scene::DecodedJocBindingUnavailable::DecodedJocObjectPopulationMismatch { .. })
    ));
}

fn admitted_prefix() -> OamdContentPrefix {
    OamdContentPrefix {
        syntax_version: 0,
        object_count: 16,
        content: ContentDescription::DynamicOnly { lfe_present: true },
        alternate_object_data_present: false,
        element_count: 0,
        consumed_bits: 0,
    }
}

fn admitted_payload() -> OamdPayload {
    OamdPayload {
        prefix: admitted_prefix(),
        object_classes: Vec::new(),
        elements: Vec::new(),
        consumed_bits: 0,
    }
}

#[test]
fn scene_builder_uses_actual_joc_population_and_keeps_binding_carrier_local() {
    let payload = admitted_payload();
    let layout =
        openjoc_scene::ProgrammeLayout::from_prefix(&payload.prefix).expect("admitted layout");
    let mut builder =
        openjoc_scene::SceneBuilder::new(48_000, &payload.prefix).expect("scene builder");
    let rows = vec![vec![0.0; 2]; 15];
    builder
        .append_frame_with_layout_and_binding_profile(
            &rows,
            Some(&[0.0, 0.0]),
            &payload,
            None,
            &layout,
            15,
            BindingCodecProfile::EAc3JocObservedOrdinary,
        )
        .expect("admitted frame");
    let scene = builder.finish().expect("scene");
    assert_eq!(
        scene.semantic_binding,
        openjoc_scene::SemanticBindingState::ResolvedWithinCarrier
    );
    assert!(scene.require_authored_object_audio_binding().is_err());

    let mut builder =
        openjoc_scene::SceneBuilder::new(48_000, &payload.prefix).expect("scene builder");
    builder
        .append_frame_with_layout_and_binding_profile(
            &rows,
            Some(&[0.0, 0.0]),
            &payload,
            None,
            &layout,
            14,
            BindingCodecProfile::EAc3JocObservedOrdinary,
        )
        .expect("structural frame");
    assert_eq!(
        builder.finish().expect("scene").semantic_binding,
        openjoc_scene::SemanticBindingState::Unresolved
    );
}

#[test]
fn scene_builder_rejects_a_binding_profile_change_within_one_programme() {
    let payload = admitted_payload();
    let layout =
        openjoc_scene::ProgrammeLayout::from_prefix(&payload.prefix).expect("admitted layout");
    let rows = vec![vec![0.0; 2]; 15];
    let mut builder =
        openjoc_scene::SceneBuilder::new(48_000, &payload.prefix).expect("scene builder");

    builder
        .append_frame_with_layout_and_binding_profile(
            &rows,
            Some(&[0.0, 0.0]),
            &payload,
            None,
            &layout,
            15,
            BindingCodecProfile::EAc3JocObservedOrdinary,
        )
        .expect("first frame");
    builder
        .append_frame_with_layout_and_binding_profile(
            &rows,
            Some(&[0.0, 0.0]),
            &payload,
            None,
            &layout,
            15,
            BindingCodecProfile::EAc3JocObservedOrdinaryCompatWarp3,
        )
        .expect("second frame");

    assert_eq!(
        builder.finish().expect("scene").semantic_binding,
        openjoc_scene::SemanticBindingState::Unresolved
    );
}

#[test]
fn streaming_scene_builder_validates_admitted_binding_without_captured_basis() {
    let payload = admitted_payload();
    let layout =
        openjoc_scene::ProgrammeLayout::from_prefix(&payload.prefix).expect("admitted layout");
    let rows = vec![vec![0.0; 2]; 15];
    let mut builder = openjoc_scene::SceneBuilder::new_streaming(48_000, &payload.prefix)
        .expect("streaming scene builder");

    builder
        .append_frame_with_layout_and_binding_profile(
            &rows,
            Some(&[0.0, 0.0]),
            &payload,
            None,
            &layout,
            15,
            BindingCodecProfile::EAc3JocObservedOrdinary,
        )
        .expect("admitted streaming frame");

    let summary = builder.finish_streaming().expect("streaming summary");
    assert_eq!(summary.max_reconstruction_rows, 15);
    assert_eq!(summary.object_count, 16);
}
