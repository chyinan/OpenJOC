use openjoc_api::{
    BinauralConfig, BuiltinHrtf, ListenerOrientation, OpenJocConfig, OpenJocSession, RenderMode,
};

#[test]
fn system_h_public_sessions_prepare_apply_and_reset_d1_and_d2() {
    for hrtf in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "22.2".to_owned(),
            binaural: Some(BinauralConfig::builtin(hrtf, "22.2")),
            ..OpenJocConfig::default()
        };
        for pull in [false, true] {
            let mut session = if pull {
                OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128)
            } else {
                OpenJocSession::new_with_listener_orientation(config.clone())
            }
            .unwrap();
            let preparer = session.listener_orientation_preparer().unwrap();
            assert_eq!(preparer.source_count(), 22);
            let identity = preparer
                .prepare(ListenerOrientation::IDENTITY, 0, 1)
                .unwrap();
            assert_eq!(identity.kernels().len(), 22);
            assert_eq!(
                session
                    .apply_prepared_listener_orientation(identity)
                    .unwrap()
                    .accepted_sequence,
                1
            );
            let angle = 9.0_f64.to_radians() / 2.0;
            let pose = ListenerOrientation::new(0.0, 0.0, angle.sin(), angle.cos()).unwrap();
            let update = preparer.prepare(pose, 0, 2).unwrap();
            assert_eq!(
                session
                    .apply_prepared_listener_orientation(update)
                    .unwrap()
                    .accepted_sequence,
                2
            );
            session.reset();
            assert_eq!(session.listener_orientation_stream_epoch(), Some(1));
        }
    }
}
