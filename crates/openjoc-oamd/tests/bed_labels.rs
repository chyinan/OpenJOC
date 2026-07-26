use openjoc_oamd::{
    BedAssignment, ContentDescription, IsfLabel, IsfRing, OamdContentPrefix, ObjectAnchor,
    SpeakerLabel,
};

#[test]
fn expands_standard_bed_mask_in_normative_bit_index_order() {
    use SpeakerLabel::{
        RcC, RcL, RcLb, RcLfe, RcLfe2, RcLs, RcLw, RcR, RcRb, RcRs, RcRw, RcTbl, RcTbr, RcTfl,
        RcTfr, RcTsl, RcTsr,
    };

    assert_eq!(
        BedAssignment::Standard(0x03ff).speaker_labels(),
        Ok(vec![
            RcLfe2, RcLw, RcRw, RcTbl, RcTbr, RcTsl, RcTsr, RcTfl, RcTfr, RcLb, RcRb, RcLs, RcRs,
            RcLfe, RcC, RcL, RcR,
        ])
    );
    assert_eq!(BedAssignment::LfeOnly.speaker_labels(), Ok(vec![RcLfe]));
}

#[test]
fn expands_complete_content_description_in_bed_isf_dynamic_order() {
    let prefix = OamdContentPrefix {
        syntax_version: 0,
        object_count: 8,
        content: ContentDescription::Mixed {
            bed_channel_distribute: Some(false),
            beds: vec![BedAssignment::Standard(1 << 8)],
            intermediate_spatial_format: Some(0),
            dynamic_objects: Some(3),
        },
        alternate_object_data_present: false,
        element_count: 1,
        consumed_bits: 0,
    };

    assert_eq!(
        prefix.object_anchors(),
        Ok(vec![
            ObjectAnchor::Speaker(SpeakerLabel::RcC),
            ObjectAnchor::IntermediateSpatial(IsfLabel {
                ring: IsfRing::Middle,
                index: 1,
            }),
            ObjectAnchor::IntermediateSpatial(IsfLabel {
                ring: IsfRing::Middle,
                index: 2,
            }),
            ObjectAnchor::IntermediateSpatial(IsfLabel {
                ring: IsfRing::Middle,
                index: 3,
            }),
            ObjectAnchor::IntermediateSpatial(IsfLabel {
                ring: IsfRing::Upper,
                index: 1,
            }),
            ObjectAnchor::Dynamic,
            ObjectAnchor::Dynamic,
            ObjectAnchor::Dynamic,
        ])
    );
}

#[test]
fn expands_nonstandard_bed_mask_in_normative_bit_index_order() {
    use SpeakerLabel::{
        RcC, RcL, RcLb, RcLfe, RcLfe2, RcLs, RcLw, RcR, RcRb, RcRs, RcRw, RcTbl, RcTbr, RcTfl,
        RcTfr, RcTsl, RcTsr,
    };

    assert_eq!(
        BedAssignment::Nonstandard(0x1ffff).speaker_labels(),
        Ok(vec![
            RcLfe2, RcRw, RcLw, RcTbr, RcTbl, RcTsr, RcTsl, RcTfr, RcTfl, RcRb, RcLb, RcRs, RcLs,
            RcLfe, RcC, RcR, RcL,
        ])
    );
}
