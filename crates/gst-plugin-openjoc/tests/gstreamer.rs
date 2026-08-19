#![cfg(feature = "gstreamer")]

use gst::prelude::*;

mod null_sink {
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Imp;

    #[glib::object_subclass]
    impl glib::subclass::prelude::ObjectSubclass for Imp {
        const NAME: &'static str = "OpenJocTestNullSink";
        type Type = NullSink;
        type ParentType = gst_base::BaseSink;
    }

    impl glib::subclass::prelude::ObjectImpl for Imp {}
    impl gst::subclass::prelude::GstObjectImpl for Imp {}

    impl gst::subclass::prelude::ElementImpl for Imp {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static METADATA: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
            Some(METADATA.get_or_init(|| {
                gst::subclass::ElementMetadata::new(
                    "OpenJOC test null sink",
                    "Sink/Audio",
                    "Test-only sink",
                    "OpenJOC tests",
                )
            }))
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
            TEMPLATES.get_or_init(|| {
                vec![
                    gst::PadTemplate::new(
                        "sink",
                        gst::PadDirection::Sink,
                        gst::PadPresence::Always,
                        &gst::Caps::new_any(),
                    )
                    .expect("test sink pad template"),
                ]
            })
        }
    }

    impl gst_base::subclass::prelude::BaseSinkImpl for Imp {
        fn render(&self, _buffer: &gst::Buffer) -> Result<gst::FlowSuccess, gst::FlowError> {
            Ok(gst::FlowSuccess::Ok)
        }
    }

    glib::wrapper! {
        pub struct NullSink(ObjectSubclass<Imp>) @extends gst_base::BaseSink, gst::Element, gst::Object;
    }

    impl NullSink {
        pub fn new() -> Self {
            glib::Object::builder().build()
        }
    }
}

fn push_bits(bytes: &mut [u8], cursor: &mut usize, value: u64, width: usize) {
    for shift in (0..width).rev() {
        if value & (1_u64 << shift) != 0 {
            bytes[*cursor / 8] |= 0x80 >> (*cursor % 8);
        }
        *cursor += 1;
    }
}

fn ordinary_eac3_header_only_frame() -> gst::Buffer {
    let mut bytes = [0_u8; 16];
    let mut cursor = 0;
    push_bits(&mut bytes, &mut cursor, 0x0b77, 16);
    push_bits(&mut bytes, &mut cursor, 0, 2);
    push_bits(&mut bytes, &mut cursor, 0, 3);
    push_bits(&mut bytes, &mut cursor, 7, 11);
    push_bits(&mut bytes, &mut cursor, 0, 2);
    push_bits(&mut bytes, &mut cursor, 3, 2);
    gst::Buffer::from_mut_slice(bytes.to_vec())
}

#[test]
fn openjocdec_registers_at_rank_none_with_framed_eac3_sink() {
    gst::init().expect("GStreamer initializes");
    gstopenjoc::register_static_plugin().expect("OpenJOC plugin registers");

    let factory = gst::ElementFactory::find("openjocdec").expect("openjocdec factory");
    assert_eq!(factory.rank(), gst::Rank::NONE);
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
    assert_eq!(decoder.property::<String>("dialnorm"), "default");
}

#[test]
fn ordinary_eac3_is_rejected_without_pcm_or_panic() {
    gst::init().expect("GStreamer initializes");
    gstopenjoc::register_static_plugin().expect("OpenJOC plugin registers");

    let caps = gst::Caps::builder("audio/x-eac3")
        .field("framed", true)
        .field("alignment", "frame")
        .build();
    let appsrc = gst_app::AppSrc::builder()
        .caps(&caps)
        .format(gst::Format::Time)
        .build();
    let decoder = gst::ElementFactory::make("openjocdec")
        .build()
        .expect("decoder instance");
    let sink = null_sink::NullSink::new();
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([appsrc.upcast_ref(), &decoder, sink.upcast_ref()])
        .expect("pipeline add");
    gst::Element::link_many([appsrc.upcast_ref(), &decoder, sink.upcast_ref()])
        .expect("pipeline link");

    pipeline
        .set_state(gst::State::Playing)
        .expect("pipeline playing");
    appsrc
        .push_buffer(ordinary_eac3_header_only_frame())
        .expect("ordinary input accepted by appsrc");
    appsrc.end_of_stream().expect("send EOS");

    let bus = pipeline.bus().expect("pipeline bus");
    let mut saw_decode_error = false;
    for _ in 0..20 {
        let Some(message) = bus.timed_pop(gst::ClockTime::from_seconds(1)) else {
            continue;
        };
        match message.view() {
            gst::MessageView::Error(error) => {
                let text = error.error().to_string();
                assert!(text.contains("OpenJOC"), "unexpected decoder error: {text}");
                saw_decode_error = true;
                break;
            }
            gst::MessageView::Eos(..) => break,
            _ => {}
        }
    }
    pipeline
        .set_state(gst::State::Null)
        .expect("pipeline stops");
    assert!(saw_decode_error, "ordinary E-AC-3 must fail closed");
}
