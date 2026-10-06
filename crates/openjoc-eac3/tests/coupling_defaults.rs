//! Public encoder-generated regressions for ETSI TS 102 366 Table E.1.12.
use openjoc_eac3::{
    AudioPcmSynthesizer, CouplingInformation, SemanticChannel, decode_audio_blocks,
    decode_audio_frame_pcm, emit_coding_tool_inventory, parse_audio_frame,
};

const COUPLED: [&[u8]; 2] = [
    include_bytes!("fixtures/coupling/acmod5.eac3"),
    include_bytes!("fixtures/coupling/acmod6.eac3"),
];
const UNCOUPLED: [&[u8]; 2] = [
    include_bytes!("fixtures/coupling/acmod5-nocpl.eac3"),
    include_bytes!("fixtures/coupling/acmod6-nocpl.eac3"),
];

#[test]
fn decodes_nonzero_origin_default_coupling_through_all_six_blocks() {
    let dither = vec![0.0; 20_000];
    for bytes in COUPLED {
        let blocks = decode_audio_blocks(bytes, &dither).expect("valid FFmpeg silence");
        assert_eq!(blocks.len(), 6);
        for block in blocks {
            let Some(CouplingInformation::Standard(info)) = block.prefix.coupling else {
                panic!("expected standard coupling");
            };
            assert_eq!(
                (info.begin_frequency_code, info.end_frequency_code),
                (11, 12)
            );
            assert_eq!(info.subband_count, 4);
            assert_eq!(&info.band_structure[..4], &[false, false, true, true]);
            assert_eq!(info.band_count, 2);
            assert!(
                info.coordinates
                    .iter()
                    .all(|co| co.as_ref().unwrap().bands.len() == 2)
            );
        }
        let pcm = decode_audio_frame_pcm(bytes, &dither, &mut AudioPcmSynthesizer::default())
            .expect("valid coupled PCM");
        assert_eq!(pcm.channels.len(), 4);
        assert!(pcm.channels.iter().all(|channel| channel.len() == 1536
            && channel.iter().all(|sample| *sample == 0.0)));
    }
}

#[test]
fn inventory_labels_four_channel_modes_in_coded_order() {
    let dither = vec![0.0; 20_000];
    let expected = [
        [
            SemanticChannel::Left,
            SemanticChannel::Centre,
            SemanticChannel::Right,
            SemanticChannel::Other(3),
        ],
        [
            SemanticChannel::Left,
            SemanticChannel::Right,
            SemanticChannel::LeftSurround,
            SemanticChannel::RightSurround,
        ],
    ];
    for (bytes, labels) in UNCOUPLED.into_iter().zip(expected) {
        let frame = parse_audio_frame(bytes).unwrap();
        let blocks = decode_audio_blocks(bytes, &dither).unwrap();
        let inventory =
            emit_coding_tool_inventory("four-channel-silence", 0, &frame, &blocks).unwrap();
        assert_eq!(inventory.blocks.len(), 24);
        for block in inventory.blocks.chunks_exact(4) {
            assert_eq!(
                block.iter().map(|entry| entry.channel).collect::<Vec<_>>(),
                labels
            );
        }
    }
}
