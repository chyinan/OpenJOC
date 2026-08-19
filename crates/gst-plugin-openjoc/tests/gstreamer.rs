#![cfg(feature = "gstreamer")]

use gst::prelude::*;

fn generic_eac3_caps() -> gst::Caps {
    gst::Caps::builder("audio/x-eac3")
        .field("framed", true)
        .field("alignment", "frame")
        .build()
}

fn classified_eac3_caps(joc: bool) -> gst::Caps {
    let builder = gst::Caps::builder("audio/x-eac3")
        .field("framed", true)
        .field("alignment", "frame")
        .field("openjoc-joc", joc);
    if joc {
        builder.features(["openjoc:joc"]).build()
    } else {
        builder.build()
    }
}

#[test]
fn openjocdec_registers_as_a_joc_only_primary_decoder() {
    gst::init().expect("GStreamer initializes");
    gstopenjoc::register_static_plugin().expect("OpenJOC plugin registers");

    let factory = gst::ElementFactory::find("openjocdec").expect("openjocdec factory");
    assert_eq!(factory.rank(), gst::Rank::PRIMARY);
    let sink_caps = factory
        .static_pad_templates()
        .iter()
        .find(|template| template.name_template() == "sink")
        .expect("openjocdec sink template")
        .caps();
    assert!(sink_caps.to_string().contains("openjoc-joc"));
    assert!(sink_caps.to_string().contains("openjoc:joc"));
    assert!(!generic_eac3_caps().can_intersect(&sink_caps));
    assert!(!classified_eac3_caps(false).can_intersect(&sink_caps));
    assert!(classified_eac3_caps(true).can_intersect(&sink_caps));
    assert!(
        factory
            .static_pad_templates()
            .iter()
            .any(|template| template.name_template() == "sink")
    );
    assert!(
        factory
            .static_pad_templates()
            .iter()
            .any(|template| template.name_template() == "src")
    );

    let decoder = factory.create().build().expect("decoder instance creates");
    assert_eq!(decoder.property::<String>("render-mode"), "speaker");
    assert_eq!(decoder.property::<String>("speaker-layout"), "5.1");
    assert_eq!(decoder.property::<String>("virtual-layout"), "7.1.4");
    assert_eq!(decoder.property::<u8>("drc-boost"), 100);
    assert_eq!(decoder.property::<u8>("drc-cut"), 100);
    assert_eq!(decoder.property::<String>("dialnorm"), "default");
}

#[test]
fn ordinary_eac3_never_autoplugs_openjoc_even_at_high_rank() {
    gst::init().expect("GStreamer initializes");
    gstopenjoc::register_static_plugin().expect("OpenJOC plugin registers");

    let factory = gst::ElementFactory::find("openjocdec").expect("openjocdec factory");
    let old_rank = factory.rank();
    factory.set_rank(gst::Rank::PRIMARY + 10_000);
    let sink_caps = factory
        .static_pad_templates()
        .iter()
        .find(|template| template.name_template() == "sink")
        .expect("openjocdec sink template")
        .caps();
    assert!(!generic_eac3_caps().can_intersect(&sink_caps));
    assert!(!classified_eac3_caps(false).can_intersect(&sink_caps));
    factory.set_rank(old_rank);
}

#[test]
fn classifier_is_registered_above_ac3parse_and_advertises_both_results() {
    gst::init().expect("GStreamer initializes");
    gstopenjoc::register_static_plugin().expect("OpenJOC plugin registers");

    let factory = gst::ElementFactory::find("openjocclassify").expect("classifier factory");
    assert_eq!(factory.rank(), gst::Rank::PRIMARY + 2);
    let src_caps = factory
        .static_pad_templates()
        .iter()
        .find(|template| template.name_template() == "src")
        .expect("classifier src template")
        .caps();
    assert!(src_caps.can_intersect(&classified_eac3_caps(false)));
    assert!(src_caps.can_intersect(&classified_eac3_caps(true)));
    assert!(
        factory
            .metadata("klass")
            .expect("classifier metadata")
            .contains("Parser")
    );
}
