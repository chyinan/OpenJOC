//! Fresh channel exponent boundary regressions; see fixtures/coupling/FRESH_EXPONENTS.md.
//!
//! These bounded zero-BAP frames isolate structure state: every active block
//! supplies fresh coordinates, D15 coupling exponents, and D15/D25/D45 channel
//! exponents. They are not encoder-generated
//! conformance or audible-PCM vectors. Expected structures are independent test
//! inputs, not obtained from the decoder's private defaults/cache helpers.

use openjoc_eac3::{AudioPcmSynthesizer, decode_audio_blocks, decode_audio_frame_pcm};

#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push((value >> shift) & 1 != 0);
        }
    }

    fn bytes(mut self) -> Vec<u8> {
        assert!(self.0.len() < 4096 * 8);
        self.0.resize(4096 * 8, false);
        self.0
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0, |value, bit| (value << 1) | u8::from(*bit))
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
struct Active {
    begin: u8,
    /// Exclusive absolute subband end, corresponding to cplendf + 3.
    end: u8,
    explicit: bool,
    expected: &'static [bool],
}

const A: &[bool] = &[false, false, true, false, true];

fn active(explicit: bool, expected: &'static [bool]) -> Active {
    Active {
        begin: 7,
        end: 12,
        explicit,
        expected,
    }
}

fn frame(sequence: [Active; 6], strategy: u8) -> (Vec<u8>, Vec<usize>) {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent stream
    bits.push(0, 3); // substream id
    bits.push(2047, 11); // 4096 bytes
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six blocks
    bits.push(3, 3); // L/C/R: avoids stereo rematrix and phase syntax
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // compression metadata
    bits.push(0, 1); // mixing metadata
    bits.push(0, 1); // informational metadata
    bits.push(0, 1); // addbsi
    bits.push(1, 1); // explicit exponent strategies
    bits.push(0, 1); // no AHT
    bits.push(0, 2); // frame SNR offsets
    bits.push(0, 1); // no transient processing
    bits.push(0, 7); // optional block syntax absent
    for index in 0..6 {
        if index != 0 {
            bits.push(1, 1);
        } // cplstre
        bits.push(1, 1); // cplinu
    }
    for _ in 0..6 {
        bits.push(1, 2); // coupling D15
        for _ in 0..3 {
            bits.push(u64::from(strategy), 2);
        } // fresh channel exponents
    }
    for _ in 0..3 {
        bits.push(0, 5);
    } // converter exponent strategies
    bits.push(0, 6); // coarse SNR: zero BAP
    bits.push(0, 4); // fine SNR
    bits.push(0, 1); // no block-start information

    let mut ends = Vec::new();
    for (index, config) in sequence.iter().enumerate() {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // block zero spxinu; later spxstre (remain off)
        {
            assert_eq!(
                config.expected.len(),
                usize::from(config.end - config.begin)
            );
            assert!(!config.expected[0]);
            bits.push(0, 1); // standard coupling
            bits.push(7, 3); // all three channels coupled
            bits.push(u64::from(config.begin), 4);
            bits.push(u64::from(config.end - 3), 4);
            bits.push(u64::from(config.explicit), 1);
            if config.explicit {
                for merged in &config.expected[1..] {
                    bits.push(u64::from(*merged), 1);
                }
            }
            let bands = config.expected.iter().filter(|merged| !**merged).count();
            for channel in 0..3 {
                if index != 0 {
                    bits.push(1, 1);
                } // cplcoe: fresh coordinates
                bits.push(channel, 2); // distinct master per channel
                for band in 0..bands {
                    bits.push((index + band + 1) as u64, 4);
                    bits.push(channel + 4, 4);
                }
            }
        }
        {
            bits.push(5, 4); // coupling exponent 10
            for _ in 0..4 * (config.end - config.begin) {
                bits.push(62, 7);
            }
        }
        let end = 37 + 12 * usize::from(config.begin);
        for channel in 0..3 {
            bits.push(10, 4); // channel absolute exponent
            for _ in 0..(end - 1) / (3 << (strategy - 1)) {
                bits.push(62, 7);
            } // neutral D15
            bits.push(channel, 2); // gain range marker after coordinates/exponents
        }
        bits.push(1, 1); // converter SNR marker present
        bits.push(0x155 + index as u64, 10);
        {
            if index != 0 {
                bits.push(1, 1);
            } // fresh coupling leak
            bits.push(3, 3);
            bits.push(5, 3);
        }
        ends.push(bits.0.len()); // no mantissa bits at zero BAP
    }
    let mut bytes = bits.bytes();
    // E-AC-3 CRC16 over all bytes following the syncword, including crc2.
    let mut crc = 0u16;
    for &byte in &bytes[2..4094] {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x8000 != 0 { 0x8005 } else { 0 };
        }
    }
    bytes[4094..].copy_from_slice(&crc.to_be_bytes());
    (bytes, ends)
}

fn check(sequence: [Active; 6], strategy: u8) {
    let (bytes, ends) = frame(sequence, strategy);
    let dither = vec![0.0; 20_000];
    let blocks = decode_audio_blocks(&bytes, &dither).expect("valid fresh exponent frame");
    assert_eq!(blocks.len(), 6);
    for (index, block) in blocks.iter().enumerate() {
        assert_eq!(block.prefix.next_offset_bits, ends[index]);
        assert_eq!(block.mantissa_end_offset_bits, ends[index]);
        assert_eq!(
            block.prefix.converter_snr_offset,
            Some(0x155 + index as u16)
        );
        for (channel, exponents) in block.prefix.channel_exponents.iter().enumerate() {
            let exponents = exponents.as_ref().unwrap();
            assert_eq!(exponents.strategy, strategy);
            assert_eq!(exponents.start_mantissa, 0);
            assert_eq!(exponents.end_mantissa, 121);
            assert_eq!(exponents.grouped_exponents, vec![62; 40 >> (strategy - 1)]);
            assert_eq!(exponents.decoded, vec![10; 121]);
            assert_eq!(exponents.gain_range, Some(channel as u8));
            assert_eq!(block.channel_baps[channel], vec![0; 121]);
        }
        let coupling = block.prefix.coupling_exponents.as_ref().unwrap();
        assert_eq!(coupling.start_mantissa, 121);
        assert_eq!(coupling.end_mantissa, 181);
        assert_eq!(coupling.grouped_exponents.len(), 20);
    }
    let pcm = decode_audio_frame_pcm(&bytes, &dither, &mut AudioPcmSynthesizer::default())
        .expect("six blocks reconstruct PCM");
    assert_eq!(pcm.channels.len(), 3);
    assert!(pcm.channels.iter().all(|channel| channel.len() == 1536));
    assert!(
        pcm.channels
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
    assert!(pcm.lfe.is_none());
}

#[test]
fn fresh_d15_fixture_is_reproducible_and_stops_at_coupling_start() {
    let (bytes, ends) = frame([active(true, A); 6], 1);
    assert_eq!(
        bytes,
        include_bytes!("fixtures/coupling/explicit-fresh-crc.eac3")
    );
    assert_eq!(ends, [1267, 2387, 3507, 4627, 5747, 6867]);
    // Independent bit offsets from the checked-in fixture's block-1 trace.
    let read = |offset: usize, width: usize| {
        (offset..offset + width).fold(0u16, |value, bit| {
            (value << 1) | u16::from((bytes[bit / 8] >> (7 - bit % 8)) & 1)
        })
    };
    assert_eq!(read(1273, 4), 7); // cplbegf
    assert_eq!(read(1277, 4), 9); // cplendf
    assert_eq!(read(1511, 4), 10); // channel 0 absolute exponent
    assert_eq!(read(1795, 2), 0); // gainrng[0], after exactly 40 groups
    assert_eq!(read(1797, 4), 10); // channel 1 absolute exponent
    assert_eq!(read(2081, 2), 1); // gainrng[1]
    assert_eq!(read(2083, 4), 10); // channel 2 absolute exponent
    assert_eq!(read(2367, 2), 2); // gainrng[2]
    assert_eq!(read(2369, 1), 1); // convsnroffste
    assert_eq!(read(2370, 10), 342); // convsnroffst
    assert_eq!(read(2380, 1), 1); // cplleake
    assert_eq!(read(2381, 3), 3); // cplfleak
    assert_eq!(read(2384, 3), 5); // cplsleak

    check([active(true, A); 6], 1);
}

#[test]
fn fresh_d25_and_d45_stop_at_coupling_start() {
    for strategy in [2, 3] {
        check([active(true, A); 6], strategy);
    }
}

#[test]
fn fresh_exponents_with_retained_coupling_structure_stay_aligned() {
    for strategy in [1, 2, 3] {
        check(
            [
                active(true, A),
                active(false, A),
                active(false, A),
                active(true, A),
                active(false, A),
                active(false, A),
            ],
            strategy,
        );
    }
}
