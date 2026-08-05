// pattern: Functional Core

//! TS 103 420 E-AC-3 access-unit audio assembly.

use crate::{
    AccessUnitIndex, AudioPcmSynthesizer, BitstreamInformation, DecodedAudioPcm, Eac3Error,
    InternalBasePolicy, StreamType, SyncframeIndexEntry, decode_audio_frame_pcm_with_policy,
};

/// Channel-major PCM and timing emitted by one JOC elementary-stream access unit.
///
/// Full-bandwidth channels are ordered as TS 103 420 Table 47: L, R, C, Ls,
/// Rs, followed by the optional 7.X or 5.X+2 pair. LFE is retained separately
/// because the JOC tool bypasses it.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAccessUnitPcm {
    pub sample_rate: u32,
    pub samples: u16,
    pub channels: Vec<Vec<f64>>,
    pub lfe: Option<Vec<f64>>,
}

/// Stateful decoder for the one-I0/optional-D0 JOC elementary-stream shape.
///
/// TS 103 420 E.3 restricts a conforming JOC elementary stream to one
/// independent substream (I0) and at most one dependent substream (D0). The
/// dependent channel data replaces matching I0 locations and supplements the
/// base 5.X channels for the 7.X and 5.X+2 configurations. Transform delay is
/// retained independently for I0 and D0 across access units.
#[derive(Clone, Debug, Default)]
pub struct JocAccessUnitPcmDecoder {
    independent: AudioPcmSynthesizer,
    dependent: AudioPcmSynthesizer,
    dependent_present: bool,
}

impl JocAccessUnitPcmDecoder {
    /// Creates a decoder with zero TDAC history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all substream TDAC history.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Decodes and assembles one indexed JOC access unit.
    ///
    /// `dither_values` is a deterministic sequence supplied by the caller and
    /// is reused from the start for each source substream. This keeps the
    /// normative decoder boundary pure while permitting any ETSI-allowed
    /// random dither sequence to be injected by an application.
    ///
    /// # Errors
    /// Returns a checked indexing, channel-map, audio-block, transform, or
    /// access-unit PCM alignment error. Decoder state is committed only after
    /// both source frames and the channel merge succeed.
    pub fn decode(
        &mut self,
        stream: &[u8],
        frames: &[SyncframeIndexEntry],
        unit: AccessUnitIndex,
        dither_values: &[f64],
    ) -> Result<DecodedAccessUnitPcm, Eac3Error> {
        self.decode_with_policy(
            stream,
            frames,
            unit,
            dither_values,
            InternalBasePolicy::CurrentDefault,
        )
    }

    /// Decodes and assembles one access unit with an explicit internal-base
    /// presentation policy. The default [`Self::decode`] behavior is kept
    /// unchanged for existing callers.
    pub fn decode_with_policy(
        &mut self,
        stream: &[u8],
        frames: &[SyncframeIndexEntry],
        unit: AccessUnitIndex,
        dither_values: &[f64],
        policy: InternalBasePolicy,
    ) -> Result<DecodedAccessUnitPcm, Eac3Error> {
        let unit_end = unit
            .first_frame
            .checked_add(unit.frame_count)
            .ok_or(Eac3Error::InvalidAccessUnitRange)?;
        if unit.frame_count == 0 || unit_end > frames.len() {
            return Err(Eac3Error::InvalidAccessUnitRange);
        }
        let first = frames[unit.first_frame];
        if first.header.stream_type != StreamType::Independent || first.header.substream_id != 0 {
            return Err(Eac3Error::MissingIndependentSubstreamZero {
                frame: unit.first_frame,
            });
        }
        if unit.frame_count > 2 {
            return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
                actual: unit.frame_count,
            });
        }
        let dependent_entry = if unit.frame_count == 2 {
            let entry = frames[unit.first_frame + 1];
            if entry.header.stream_type != StreamType::Dependent || entry.header.substream_id != 0 {
                return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
                    actual: unit.frame_count,
                });
            }
            Some(entry)
        } else {
            None
        };
        if first.header.sample_rate != unit.sample_rate || first.header.samples != unit.samples {
            return Err(Eac3Error::SubstreamTimingMismatch {
                frame: unit.first_frame,
            });
        }
        if first.header.audio_blocks != 6 {
            return Err(Eac3Error::UnsupportedJocAudioBlockCount {
                actual: first.header.audio_blocks,
            });
        }

        let mut independent_synth = self.independent.clone();
        let mut dependent_synth = self.dependent.clone();
        if dependent_entry.is_some() != self.dependent_present {
            dependent_synth.reset();
        }
        let (independent_info, independent) =
            decode_frame(stream, first, dither_values, &mut independent_synth, policy)?;
        let dependent = dependent_entry
            .map(|entry| decode_frame(stream, entry, dither_values, &mut dependent_synth, policy))
            .transpose()?;
        if let Some((info, _)) = &dependent
            && (info.header.sample_rate != unit.sample_rate || info.header.samples != unit.samples)
        {
            return Err(Eac3Error::SubstreamTimingMismatch {
                frame: unit.first_frame + 1,
            });
        }
        if let Some((info, _)) = &dependent
            && info.header.audio_blocks != 6
        {
            return Err(Eac3Error::UnsupportedJocAudioBlockCount {
                actual: info.header.audio_blocks,
            });
        }
        let output = merge_substreams(
            unit,
            &independent_info,
            independent,
            dependent.as_ref().map(|(info, pcm)| (info, pcm)),
        )?;

        self.independent = independent_synth;
        self.dependent = dependent_synth;
        self.dependent_present = dependent_entry.is_some();
        Ok(output)
    }
}

fn decode_frame(
    stream: &[u8],
    entry: SyncframeIndexEntry,
    dither_values: &[f64],
    synthesizer: &mut AudioPcmSynthesizer,
    policy: InternalBasePolicy,
) -> Result<(BitstreamInformation, DecodedAudioPcm), Eac3Error> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let bytes = stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })?;
    let info = crate::parse_audio_frame(bytes)?.bsi;
    let pcm = decode_audio_frame_pcm_with_policy(bytes, dither_values, synthesizer, policy)?;
    Ok((info, pcm))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ChannelLocation {
    Left,
    Right,
    Centre,
    LeftSurround,
    RightSurround,
    LeftBack,
    RightBack,
    TopFrontLeft,
    TopFrontRight,
    Other(u8),
    Lfe(u8),
}

fn standard_channel_locations(
    audio_coding_mode: u8,
    lfe_on: bool,
) -> Result<Vec<ChannelLocation>, Eac3Error> {
    let mut locations = match audio_coding_mode {
        0 => vec![ChannelLocation::Left, ChannelLocation::Right],
        1 => vec![ChannelLocation::Centre],
        2 => vec![ChannelLocation::Left, ChannelLocation::Right],
        3 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
        ],
        4 => vec![
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Other(3), // Cs
        ],
        5 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
            ChannelLocation::Other(3), // Cs
        ],
        6 => vec![
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ],
        7 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ],
        _ => return Err(Eac3Error::FrameSizeOverflow),
    };
    if lfe_on {
        locations.push(ChannelLocation::Lfe(0));
    }
    Ok(locations)
}

fn channel_locations(info: &BitstreamInformation) -> Result<Vec<ChannelLocation>, Eac3Error> {
    let Some(map) = info.channel_map else {
        return standard_channel_locations(info.audio_coding_mode, info.lfe_on);
    };
    let mut locations = Vec::new();
    for bit in 0..16_u8 {
        if map & (1_u16 << (15 - bit)) == 0 {
            continue;
        }
        locations.extend(match bit {
            0 => vec![ChannelLocation::Left],
            1 => vec![ChannelLocation::Centre],
            2 => vec![ChannelLocation::Right],
            3 => vec![ChannelLocation::LeftSurround],
            4 => vec![ChannelLocation::RightSurround],
            5 => vec![ChannelLocation::Other(1), ChannelLocation::Other(2)], // Lc/Rc
            6 => vec![ChannelLocation::LeftBack, ChannelLocation::RightBack],
            7 => vec![ChannelLocation::Other(3)], // Cs
            8 => vec![ChannelLocation::Other(4)], // Ts
            9 => vec![ChannelLocation::Other(5), ChannelLocation::Other(6)], // Lsd/Rsd
            10 => vec![ChannelLocation::Other(7), ChannelLocation::Other(8)], // Lw/Rw
            11 => vec![
                ChannelLocation::TopFrontLeft,
                ChannelLocation::TopFrontRight,
            ],
            12 => vec![ChannelLocation::Other(9)], // Vhc
            13 => vec![ChannelLocation::Other(10), ChannelLocation::Other(11)], // Lts/Rts
            14 => vec![ChannelLocation::Lfe(1)],
            15 => vec![ChannelLocation::Lfe(0)],
            _ => unreachable!(),
        });
    }
    Ok(locations)
}

fn merge_substreams(
    unit: AccessUnitIndex,
    independent_info: &BitstreamInformation,
    independent: DecodedAudioPcm,
    dependent: Option<(&BitstreamInformation, &DecodedAudioPcm)>,
) -> Result<DecodedAccessUnitPcm, Eac3Error> {
    let mut channels = Vec::<(ChannelLocation, Vec<f64>)>::new();
    let independent_locations = channel_locations(independent_info)?;
    insert_channels(&mut channels, independent_locations, &independent)?;
    let mut lfe = independent.lfe.clone();
    if let Some((info, pcm)) = dependent {
        let locations = channel_locations(info)?;
        insert_channels(&mut channels, locations, pcm)?;
        if let Some(dependent_lfe) = &pcm.lfe {
            lfe = Some(dependent_lfe.clone());
        }
    }
    if channels
        .iter()
        .any(|(_, pcm)| pcm.len() != usize::from(unit.samples))
    {
        let actual = channels.first().map_or(0, |(_, pcm)| pcm.len());
        return Err(Eac3Error::AccessUnitPcmSampleCountMismatch {
            expected: usize::from(unit.samples),
            actual,
        });
    }
    if lfe
        .as_ref()
        .is_some_and(|pcm| pcm.len() != usize::from(unit.samples))
    {
        return Err(Eac3Error::AccessUnitPcmSampleCountMismatch {
            expected: usize::from(unit.samples),
            actual: lfe.as_ref().map_or(0, Vec::len),
        });
    }
    channels.sort_by_key(|(location, _)| location_rank(*location));
    Ok(DecodedAccessUnitPcm {
        sample_rate: unit.sample_rate,
        samples: unit.samples,
        channels: channels.into_iter().map(|(_, pcm)| pcm).collect(),
        lfe,
    })
}

fn insert_channels(
    target: &mut Vec<(ChannelLocation, Vec<f64>)>,
    locations: Vec<ChannelLocation>,
    pcm: &DecodedAudioPcm,
) -> Result<(), Eac3Error> {
    let full_locations = locations
        .iter()
        .filter(|location| !matches!(location, ChannelLocation::Lfe(_)))
        .count();
    let lfe_locations = locations
        .iter()
        .filter(|location| matches!(location, ChannelLocation::Lfe(_)))
        .count();
    if lfe_locations > 1 {
        return Err(Eac3Error::MultipleLfeChannels);
    }
    if full_locations != pcm.channels.len() || lfe_locations != usize::from(pcm.lfe.is_some()) {
        return Err(Eac3Error::InvalidDependentChannelMap {
            expected: locations.len(),
            actual: pcm.channels.len() + usize::from(pcm.lfe.is_some()),
        });
    }
    let mut channel_index = 0;
    for location in locations {
        if matches!(location, ChannelLocation::Lfe(_)) {
            continue;
        }
        let data = pcm.channels[channel_index].clone();
        channel_index += 1;
        if let Some(existing) = target.iter_mut().find(|(current, _)| *current == location) {
            existing.1 = data;
        } else {
            target.push((location, data));
        }
    }
    Ok(())
}

fn location_rank(location: ChannelLocation) -> (u8, u8) {
    match location {
        ChannelLocation::Left => (0, 0),
        ChannelLocation::Right => (1, 0),
        ChannelLocation::Centre => (2, 0),
        ChannelLocation::LeftSurround => (3, 0),
        ChannelLocation::RightSurround => (4, 0),
        ChannelLocation::LeftBack => (5, 0),
        ChannelLocation::RightBack => (6, 0),
        ChannelLocation::TopFrontLeft => (7, 0),
        ChannelLocation::TopFrontRight => (8, 0),
        ChannelLocation::Other(value) => (9, value),
        ChannelLocation::Lfe(value) => (10, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(audio_coding_mode: u8, channel_map: Option<u16>) -> BitstreamInformation {
        BitstreamInformation {
            header: crate::SyncframeHeader {
                stream_type: StreamType::Independent,
                substream_id: 0,
                frame_size: 0,
                sample_rate: 48_000,
                audio_blocks: 1,
                samples: 1,
            },
            audio_coding_mode,
            lfe_on: false,
            bitstream_id: 16,
            channel_map,
            addbsi: None,
        }
    }

    #[test]
    fn dependent_pair_supplements_the_independent_five_channel_order() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![6.0], vec![7.0]],
            lfe: None,
        };
        let dependent_info = info(2, Some(1 << 9)); // custom bit 6: Lb/Rb
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("valid 7.X channel merge");
        assert_eq!(
            output.channels,
            vec![
                vec![1.0],
                vec![3.0],
                vec![2.0],
                vec![4.0],
                vec![5.0],
                vec![6.0],
                vec![7.0]
            ]
        );
    }

    #[test]
    fn dependent_standard_channels_replace_matching_independent_channels() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0], vec![8.0]],
            lfe: None,
        };
        let dependent_info = info(2, None);
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("valid replacement merge");
        assert_eq!(
            output.channels,
            vec![vec![9.0], vec![8.0], vec![2.0], vec![4.0], vec![5.0]]
        );
    }

    #[test]
    fn standard_mono_surround_and_custom_cs_share_a_location() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0], vec![8.0], vec![7.0]],
            lfe: None,
        };
        let dependent_info = info(4, None);
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(5, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("standard S channel replacement");
        assert_eq!(
            output.channels,
            vec![vec![9.0], vec![8.0], vec![2.0], vec![7.0]]
        );
    }
}
