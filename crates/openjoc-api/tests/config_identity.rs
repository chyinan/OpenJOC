use openjoc_api::{OpenJocConfig, OpenJocPacket, OpenJocSession};
use openjoc_scene::{SpeakerGeometry, SpeakerLayout};

fn custom_config(lfe: bool) -> OpenJocConfig {
    let third = if lfe {
        SpeakerGeometry::lfe("C", 0.0, -20.0)
    } else {
        SpeakerGeometry::full_range("C", 0.0, -20.0)
    };
    OpenJocConfig::default().with_speaker_layout(
        SpeakerLayout::custom(
            "studio",
            vec![
                SpeakerGeometry::full_range("A", -42.0, 0.0),
                SpeakerGeometry::full_range("B", 51.0, 9.0),
                third,
            ],
        )
        .unwrap(),
    )
}

fn render(config: OpenJocConfig) -> Vec<f32> {
    let mut session = OpenJocSession::new(config).unwrap();
    let labels = session.output_info().channel_labels;
    let mut pcm = Vec::new();
    for au in include_bytes!("fixtures/timestamps.ec3")
        .chunks_exact(4096)
        .take(6)
    {
        session
            .push_packet(OpenJocPacket {
                data: au,
                pts_samples: None,
                discontinuity: false,
                preroll: false,
            })
            .unwrap();
        while let Some(frame) = session.receive_frame() {
            assert_eq!(frame.channel_labels, labels);
            pcm.extend(frame.interleaved_f32);
        }
    }
    session.drain().unwrap();
    while let Some(frame) = session.receive_frame() {
        assert_eq!(frame.channel_labels, labels);
        pcm.extend(frame.interleaved_f32);
    }
    pcm
}

fn custom_labels(labels: [&str; 2]) -> OpenJocConfig {
    OpenJocConfig::default().with_speaker_layout(
        SpeakerLayout::custom(
            "studio",
            vec![
                SpeakerGeometry::full_range(labels[0], -42.0, 0.0),
                SpeakerGeometry::full_range(labels[1], 51.0, 9.0),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn custom_channel_label_boundaries_distinguish_identity_without_changing_pcm() {
    let first = custom_labels(["A,B", "C"]);
    let second = custom_labels(["A", "B,C"]);
    first.validate().unwrap();
    second.validate().unwrap();
    assert_ne!(
        first.effective_config_descriptor(),
        second.effective_config_descriptor()
    );
    assert_ne!(
        first.effective_config_fingerprint(),
        second.effective_config_fingerprint()
    );
    let first_pcm = render(first);
    let second_pcm = render(second);
    assert_eq!(first_pcm.len(), 18_496);
    assert_eq!(
        first_pcm
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>(),
        second_pcm
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
}

#[test]
fn custom_channel_labels_are_ordered_and_utf8_byte_length_framed() {
    let cases = [
        (["A", "B"], "2:1:A:1:B"),
        (["B", "A"], "2:1:B:1:A"),
        (["A,B", "C"], "2:3:A,B:1:C"),
        (["A", "B,C"], "2:1:A:3:B,C"),
        (["A:1:B", "C"], "2:5:A:1:B:1:C"),
        (["A", "B:1:C"], "2:1:A:5:B:1:C"),
        (["左,右", "C:=\\"], "2:7:左,右:4:C:=\\"),
    ];
    let mut fingerprints = std::collections::HashSet::new();
    for (labels, framed) in cases {
        let config = custom_labels(labels);
        config.validate().unwrap();
        assert!(config.effective_config_descriptor().contains(&format!(
            "\ncustom_layout_channels={framed}\ncustom_layout_roles="
        )));
        let fingerprint = config.effective_config_fingerprint();
        assert_eq!(fingerprint, config.clone().effective_config_fingerprint());
        assert!(fingerprints.insert(fingerprint));
    }
}

#[test]
fn custom_roles_distinguish_effective_fingerprints_and_rendered_pcm() {
    let full_range = custom_config(false);
    let lfe = custom_config(true);
    full_range.validate().unwrap();
    lfe.validate().unwrap();
    assert_eq!(
        full_range.effective_config_descriptor(),
        custom_config(false).effective_config_descriptor()
    );
    assert_eq!(
        full_range.effective_config_fingerprint(),
        full_range.clone().effective_config_fingerprint()
    );
    assert!(
        full_range
            .effective_config_descriptor()
            .contains("custom_layout_roles=full_range,full_range,full_range")
    );
    assert!(
        lfe.effective_config_descriptor()
            .contains("custom_layout_roles=full_range,full_range,lfe")
    );
    assert_ne!(
        full_range.effective_config_fingerprint(),
        lfe.effective_config_fingerprint()
    );
    let first = render(full_range);
    let second = render(lfe);
    assert_eq!(first.len(), second.len());
    assert!(
        first
            .iter()
            .zip(second)
            .any(|(left, right)| (*left - right).abs() > 0.01)
    );
}

#[test]
fn ordinary_preset_descriptor_remains_unchanged() {
    assert_eq!(
        OpenJocConfig::default().effective_config_descriptor(),
        "openjoc-effective-config-v1\nrender_mode=speaker\nlayout=5.1\ndownmix=auto\ndrc=line\ndialnorm=default\nvalidation_profile=auto\noamd_trim_configuration_count=9\nfinal_linked_gain=enabled"
    );
}

#[test]
fn custom_route_fingerprint_is_order_independent_and_gain_sensitive() {
    use openjoc_scene::{FixedRouteKey, SpatialDescriptor, SpatialRouteVector};
    let key = FixedRouteKey::new(6, 5).unwrap();
    let first = SpatialRouteVector::fixed(key, vec![0.5, 0.25, 0.0]);
    let other = SpatialRouteVector {
        identity: "other:route".to_owned(),
        vector: vec![0.0, 1.0, 0.0],
    };
    let layout = custom_config(false).speaker_layout_definition.unwrap();
    let original = layout
        .with_route_vectors(vec![first.clone(), other.clone()])
        .unwrap();
    let reordered = layout
        .with_route_vectors(vec![other.clone(), first])
        .unwrap();
    let changed = layout
        .with_route_vectors(vec![
            SpatialRouteVector::fixed(key, vec![0.75, 0.25, 0.0]),
            other,
        ])
        .unwrap();
    let descriptor = SpatialDescriptor::fixed(key, vec![0.0; 3]);
    assert_ne!(
        original.spatial().project(&descriptor).unwrap(),
        changed.spatial().project(&descriptor).unwrap()
    );
    let config = |layout| OpenJocConfig::default().with_speaker_layout(layout);
    assert_eq!(
        config(original.clone()).effective_config_fingerprint(),
        config(reordered).effective_config_fingerprint()
    );
    assert_ne!(
        config(original).effective_config_fingerprint(),
        config(changed).effective_config_fingerprint()
    );
}

fn assert_custom_name_preserves_spatial_rendering(count: usize) {
    let speakers = [
        SpeakerGeometry::full_range("FL", 75.0, 0.0),
        SpeakerGeometry::full_range("FR", -65.0, 0.0),
        SpeakerGeometry::full_range("FC", 0.0, 15.0),
    ];
    let config = |name| {
        OpenJocConfig::default()
            .with_speaker_layout(SpeakerLayout::custom(name, speakers[..count].to_vec()).unwrap())
    };
    let expected = render(config("custom-spatial"));
    let actual = render(config("2.0"));
    assert_ne!(actual, [] as [f32; 0]);
    assert_eq!(actual.len() % count, 0);
    assert_eq!(
        actual
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|sample| sample.to_bits())
            .collect::<Vec<_>>()
    );
}

#[test]
fn custom_stereo_display_name_preserves_two_channel_spatial_rendering() {
    assert_custom_name_preserves_spatial_rendering(2);
}

#[test]
fn custom_stereo_display_name_preserves_three_channel_spatial_rendering() {
    assert_custom_name_preserves_spatial_rendering(3);
}
