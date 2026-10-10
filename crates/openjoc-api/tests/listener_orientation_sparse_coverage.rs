// Functional Core: sparse SOFA preparation must reject an entire uncovered pose.
#[path = "../../openjoc-sofa/tests/support/sparse.rs"]
mod sparse;

use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, ListenerOrientation, ListenerOrientationPrepareError,
    OpenJocConfig, OpenJocSession, RenderMode,
};

#[test]
fn sparse_sofa_session_rejects_antipodal_pose_without_queuing_an_update() {
    let mut wrongly_accepted = 0;
    for sign in [-1.0, 1.0] {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "2.0".into(),
            binaural: Some(BinauralConfig::from_sofa_bytes(
                sparse::cap_fixture(sign * 20.0, true),
                "2.0",
                BinauralLfePolicy::Exclude,
            )),
            ..OpenJocConfig::default()
        };
        for pull in [false, true] {
            let mut session = if pull {
                OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128)
            } else {
                OpenJocSession::new_with_listener_orientation(config.clone())
            }
            .unwrap();
            assert_eq!(session.output_info().channel_count, 2);
            let preparer = session.listener_orientation_preparer().unwrap();
            let epoch = session.listener_orientation_stream_epoch().unwrap();
            let identity = preparer
                .prepare(ListenerOrientation::IDENTITY, epoch, 1)
                .unwrap();
            session
                .apply_prepared_listener_orientation(identity)
                .unwrap();
            assert_eq!(session.pending_listener_orientation_sequence(), Some(1));
            let uncovered = ListenerOrientation::new(
                sign * 0.5,
                sign * 0.5,
                0.0,
                std::f64::consts::FRAC_1_SQRT_2,
            )
            .unwrap();
            let direction = uncovered.world_to_listener([
                -std::f64::consts::FRAC_1_SQRT_2,
                std::f64::consts::FRAC_1_SQRT_2,
                0.0,
            ]);
            assert!(direction[2] * sign < 0.0);
            let covered = ListenerOrientation::new(
                sign * -0.5,
                sign * -0.5,
                0.0,
                std::f64::consts::FRAC_1_SQRT_2,
            )
            .unwrap();
            let update = preparer.prepare(covered, epoch, 2).unwrap();
            assert_eq!(update.kernels().len(), 2);
            for (kernel, expected) in update.kernels().iter().zip([
                [0x4000_0000_0000_0000, 0, 0x3ff8_0000_0000_0001, 0],
                [5.0_f64.to_bits(), 0, 3.75_f64.to_bits(), 0],
            ]) {
                let actual = kernel
                    .pair()
                    .left_taps()
                    .iter()
                    .chain(kernel.pair().right_taps())
                    .map(|tap| tap.to_bits())
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected);
            }
            session.apply_prepared_listener_orientation(update).unwrap();
            assert_eq!(session.pending_listener_orientation_sequence(), Some(2));
            match preparer.prepare(uncovered, epoch, 3) {
                Err(ListenerOrientationPrepareError::HrirResolutionFailure {
                    source_id: 1,
                    listener_direction,
                    reason,
                    ..
                }) => {
                    assert!(listener_direction[2] * sign < 0.0);
                    assert!(
                        reason.contains("no local spherical segment/triangle contains the request")
                    );
                }
                Ok(_) => wrongly_accepted += 1,
                Err(error) => panic!("unexpected preparation failure: {error}"),
            }
            assert_eq!(session.pending_listener_orientation_sequence(), Some(2));
            // FL remains exactly covered; FR fails after the first kernel has
            // been built locally, so no partial prepared set may escape.
            let second_uncovered = ListenerOrientation::new(
                sign * 0.5,
                sign * -0.5,
                0.0,
                std::f64::consts::FRAC_1_SQRT_2,
            )
            .unwrap();
            assert!(matches!(
                preparer.prepare(second_uncovered, epoch, 3),
                Err(ListenerOrientationPrepareError::HrirResolutionFailure { source_id: 2, .. })
            ));
            assert_eq!(session.pending_listener_orientation_sequence(), Some(2));
            let repeated = preparer.prepare(covered, epoch, 3).unwrap();
            session
                .apply_prepared_listener_orientation(repeated)
                .unwrap();
            assert_eq!(session.pending_listener_orientation_sequence(), Some(3));
        }
    }
    assert_eq!(
        wrongly_accepted, 0,
        "no partial update for an uncovered pose"
    );
}
