//! Public-decoder synthetic syntax regressions for standard coupling retention.
//!
//! These bounded zero-BAP frames isolate structure state: every block supplies fresh
//! coupling D15 exponents. Block two reuses coordinates; other blocks refresh
//! them. Channel exponents are reused after block zero. They are not encoder-generated
//! conformance or audible-PCM vectors. Expected structures are independent test
//! inputs, not obtained from the decoder's private defaults/cache helpers.

use openjoc_eac3::{CouplingInformation, decode_audio_blocks};

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
const B: &[bool] = &[false, true, false, false, false];
const DEFAULT: &[bool] = &[false, true, false, true, true];

fn active(explicit: bool, expected: &'static [bool]) -> Active {
    Active {
        begin: 7,
        end: 12,
        explicit,
        expected,
    }
}

fn frame(sequence: [Active; 6]) -> (Vec<u8>, Vec<usize>) {
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
    for index in 0..6 {
        bits.push(1, 2); // coupling D15
        for _ in 0..3 {
            bits.push(u64::from(index == 0), 2);
        } // first channel D15, then exponent reuse
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
                    bits.push(u64::from(index != 2), 1);
                } // cplcoe: block two reuses coordinates
                if index == 2 {
                    continue;
                }
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
        if index == 0 {
            let end = 37 + 12 * usize::from(config.begin);
            for channel in 0..3 {
                bits.push(10, 4); // channel absolute exponent
                for _ in 0..(end - 1) / 3 {
                    bits.push(62, 7);
                } // neutral D15
                bits.push(channel, 2); // gain range marker after coordinates/exponents
            }
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
    (bits.bytes(), ends)
}

fn check(sequence: [Active; 6]) {
    let (bytes, ends) = frame(sequence);
    let blocks = decode_audio_blocks(&bytes, &vec![0.0; 20_000]).expect("synthetic coupling frame");
    assert_eq!(blocks.len(), 6);
    for (index, (block, expected)) in blocks.iter().zip(sequence).enumerate() {
        assert_eq!(
            block.prefix.converter_snr_offset,
            Some(0x155 + index as u16)
        );
        assert_eq!(
            block.prefix.next_offset_bits, ends[index],
            "prefix alignment block {index}"
        );
        assert_eq!(
            block.mantissa_end_offset_bits, ends[index],
            "mantissa alignment block {index}"
        );
        for (channel, exponents) in block.prefix.channel_exponents.iter().enumerate() {
            assert_eq!(exponents.as_ref().unwrap().gain_range, Some(channel as u8));
        }
        let Some(CouplingInformation::Standard(info)) = &block.prefix.coupling else {
            panic!("expected standard coupling in block {index}");
        };
        assert_eq!(
            &info.band_structure[..usize::from(info.subband_count)],
            expected.expected
        );
        let bands = expected.expected.iter().filter(|merged| !**merged).count();
        assert_eq!(usize::from(info.band_count), bands);
        for (channel, coordinates) in info.coordinates.iter().enumerate() {
            let coordinates = coordinates.as_ref().unwrap();
            assert_eq!(coordinates.master, channel as u8);
            let coordinate_block = if index == 2 { 1 } else { index };
            let expected_bands = (0..bands)
                .map(|band| ((coordinate_block + band + 1) as u8, (channel + 4) as u8))
                .collect::<Vec<_>>();
            assert_eq!(coordinates.bands, expected_bands);
        }
    }
}

#[test]
fn explicit_structure_is_retained_when_omitted_and_replaced_when_present() {
    check([
        active(true, A),
        active(false, A),
        active(false, A),
        active(true, B),
        active(false, B),
        active(false, B),
    ]);
}

#[test]
fn first_active_use_and_next_frame_start_from_defaults() {
    check([active(true, A); 6]);
    check([active(false, DEFAULT); 6]);
}
