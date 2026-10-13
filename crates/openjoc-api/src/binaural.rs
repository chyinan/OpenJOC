use super::{
    AppliedBinauralUpdate, BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ, BinauralConfig, BinauralLfePolicy,
    BinauralMapping, BinauralRenderer, BinauralSourceBlock, BinauralState,
    BinauralUpdateAcceptance, BinauralUpdateApplyFailure, CartesianPosition,
    DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES, DynamicBinauralRenderer, DynamicBinauralSource,
    HrirEntryId, ListenerOrientation, ListenerOrientationPreparer,
    MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES, OpenJocError, PreparedBinauralUpdate, RenderedBlock,
    SofaLoadLimits, SourceId, StaticBinauralSource, load_builtin_hrir_f32,
    load_builtin_hrir_f32_from_asset, parse_simple_free_field_hrir, prepare_binaural_bank,
    resample_loaded_hrir_bank, resolve_hrir, resolve_hrir_f32, validate_binaural_sample_rate,
};

impl BinauralState {
    pub(super) fn new(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
    ) -> Result<Self, OpenJocError> {
        let mut lfe_delay_samples = 0;
        let (bank, mappings, lfe_index, sample_rate_hz) = if config.is_builtin() {
            let loaded = if let Some(asset) = external_asset {
                load_builtin_hrir_f32_from_asset(config.builtin_hrtf, asset)?
            } else {
                load_builtin_hrir_f32(config.builtin_hrtf)?
            };
            validate_binaural_sample_rate(loaded.metadata.sample_rate_hz)?;
            prepare_binaural_bank(
                loaded.metadata.sample_rate_hz,
                &config.virtual_layout,
                |direction| {
                    // System H includes canonical directions whose containing
                    // triangle falls outside the legacy resolver's small window.
                    // Use the same bounded coverage search as its identity pose.
                    if config.virtual_layout == "22.2" {
                        openjoc_sofa::resolve_hrir_f32_for_listener_orientation(
                            &loaded.bank,
                            direction,
                        )
                    } else {
                        resolve_hrir_f32(&loaded.bank, direction)
                    }
                    .map_err(Into::into)
                },
            )?
        } else {
            let load_limits = custom_sofa_load_limits.unwrap_or_default();
            let loaded = parse_simple_free_field_hrir(&config.sofa_bytes, load_limits)?;
            lfe_delay_samples = openjoc_sofa::hrir_resampling_delay_samples(
                loaded.bank.sample_rate_hz(),
                BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
            )?;
            let loaded = resample_loaded_hrir_bank(
                loaded,
                BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ,
                load_limits,
            )?;
            prepare_binaural_bank(
                loaded.bank.sample_rate_hz(),
                &config.virtual_layout,
                |direction| resolve_hrir(&loaded.bank, direction).map_err(Into::into),
            )?
        };
        Ok(Self {
            bank: Some(bank),
            sample_rate_hz,
            mappings,
            lfe_index,
            lfe_policy: config.lfe_policy,
            lfe_delay: openjoc_render::SampleDelay::new(lfe_delay_samples),
            engine: None,
            orientation_preparer: None,
            dynamic_engine: None,
            last_orientation_receipt: None,
            drain_started: false,
        })
    }

    #[cfg(test)]
    pub(super) fn new_with_listener_orientation(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_listener_orientation_at_epoch(
            config,
            external_asset,
            custom_sofa_load_limits,
            0,
            None,
        )
    }

    pub(super) fn new_with_listener_orientation_at_epoch(
        config: &BinauralConfig,
        external_asset: Option<&[u8]>,
        custom_sofa_load_limits: Option<SofaLoadLimits>,
        initial_stream_epoch: u64,
        shared_preparer: Option<ListenerOrientationPreparer>,
    ) -> Result<Self, OpenJocError> {
        let preparer = if let Some(preparer) = shared_preparer {
            preparer.validate_config(config)?;
            preparer
        } else {
            ListenerOrientationPreparer::new_with_resource_inputs(
                config,
                external_asset,
                custom_sofa_load_limits,
            )?
        };
        let sample_rate_hz = BUILTIN_GENERIC_HRTF_SAMPLE_RATE_HZ;
        let (lfe_index, lfe_delay_samples) = preparer.lfe_info();
        let initial = preparer
            .prepare(ListenerOrientation::IDENTITY, initial_stream_epoch, 0)
            .map_err(|error| OpenJocError::Render(error.to_string()))?;
        let dynamic_sources = initial
            .kernels()
            .iter()
            .map(|kernel| DynamicBinauralSource::new(kernel.source_id(), 1.0))
            .collect::<Result<Vec<_>, _>>()?;
        let mappings = preparer
            .source_bindings()
            .map(|(channel_index, source_id)| BinauralMapping {
                channel_index,
                source_id,
                hrir_entry: HrirEntryId::new(source_id.get()),
            })
            .collect::<Vec<_>>();
        let dynamic_engine = DynamicBinauralRenderer::new(
            sample_rate_hz,
            initial,
            dynamic_sources,
            preparer.max_filter_taps(),
            MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES,
            DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES,
        )?;
        Ok(Self {
            bank: None,
            sample_rate_hz,
            mappings,
            lfe_index,
            lfe_policy: config.lfe_policy,
            lfe_delay: openjoc_render::SampleDelay::new(lfe_delay_samples),
            engine: None,
            orientation_preparer: Some(preparer),
            dynamic_engine: Some(dynamic_engine),
            last_orientation_receipt: None,
            drain_started: false,
        })
    }

    pub(super) fn listener_orientation_enabled(&self) -> bool {
        self.dynamic_engine.is_some()
    }

    pub(super) fn listener_orientation_preparer(&self) -> Option<ListenerOrientationPreparer> {
        self.orientation_preparer.clone()
    }

    pub(super) fn listener_orientation_stream_epoch(&self) -> Option<u64> {
        self.dynamic_engine
            .as_ref()
            .map(DynamicBinauralRenderer::stream_epoch)
    }

    pub(super) fn apply_prepared_listener_orientation(
        &mut self,
        update: PreparedBinauralUpdate,
    ) -> Result<BinauralUpdateAcceptance, BinauralUpdateApplyFailure> {
        match self.dynamic_engine.as_mut() {
            Some(engine) => engine.apply_prepared(update),
            None => Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralOrientationNotEnabled,
                update,
            }),
        }
    }

    pub(super) fn last_applied_listener_orientation(&self) -> Option<AppliedBinauralUpdate> {
        self.last_orientation_receipt
    }

    pub(super) fn pending_listener_orientation_sequence(&self) -> Option<u64> {
        self.dynamic_engine
            .as_ref()
            .and_then(DynamicBinauralRenderer::pending_sequence)
    }

    pub(super) fn begin_drain(&mut self) -> Result<(), OpenJocError> {
        if let Some(engine) = self.dynamic_engine.as_mut() {
            engine.begin_drain()?;
        }
        self.drain_started = true;
        Ok(())
    }

    pub(super) fn ensure_engine(
        &mut self,
        sample_rate: u32,
    ) -> Result<&mut BinauralRenderer, OpenJocError> {
        if self.engine.is_none() {
            if self.sample_rate_hz != sample_rate {
                return Err(OpenJocError::FormatChanged {
                    expected: self.sample_rate_hz,
                    actual: sample_rate,
                });
            }
            let bank = self.bank.as_ref().ok_or_else(|| {
                OpenJocError::Render("prepared HRIR bank is unavailable".to_owned())
            })?;
            let sources = self
                .mappings
                .iter()
                .map(|mapping| {
                    let entry = bank
                        .entries()
                        .iter()
                        .find(|entry| entry.id() == mapping.hrir_entry)
                        .ok_or_else(|| OpenJocError::Render("missing prepared HRIR".to_owned()))?;
                    StaticBinauralSource::new(
                        mapping.source_id,
                        CartesianPosition::new(
                            entry.direction()[0],
                            entry.direction()[1],
                            entry.direction()[2],
                        ),
                        1.0,
                        mapping.hrir_entry,
                    )
                    .map_err(OpenJocError::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let bank = self.bank.take().ok_or_else(|| {
                OpenJocError::Render("prepared HRIR bank is unavailable".to_owned())
            })?;
            self.engine = Some(BinauralRenderer::new(sample_rate, bank, sources)?);
        }
        self.engine
            .as_mut()
            .ok_or_else(|| OpenJocError::Render("binaural engine not initialized".to_owned()))
    }

    pub(super) fn render(&mut self, frame: &RenderedBlock) -> Result<RenderedBlock, OpenJocError> {
        self.render_range(frame, 0, frame.sample_count)
    }

    pub(super) fn render_range(
        &mut self,
        frame: &RenderedBlock,
        sample_offset: usize,
        sample_count: usize,
    ) -> Result<RenderedBlock, OpenJocError> {
        let sample_rate = frame.sample_rate;
        if sample_rate != self.sample_rate_hz {
            return Err(OpenJocError::FormatChanged {
                expected: self.sample_rate_hz,
                actual: sample_rate,
            });
        }
        let range_end = sample_offset
            .checked_add(sample_count)
            .filter(|&end| end <= frame.sample_count)
            .ok_or_else(|| {
                OpenJocError::Render("binaural input slice is out of range".to_owned())
            })?;
        let logical_start_sample = frame
            .logical_start_sample
            .checked_add(sample_offset as u64)
            .ok_or_else(|| OpenJocError::Render("binaural sample index overflow".to_owned()))?;
        // Only these borrowed descriptors are temporary. Output PCM remains owned
        // by the caller; keep the FIR computation and its accumulation order intact.
        let mut storage =
            [BinauralSourceBlock::new(SourceId::new(0), &[]); openjoc_scene::MAX_CUSTOM_SPEAKERS];
        let blocks = storage.get_mut(..self.mappings.len()).ok_or_else(|| {
            OpenJocError::Render("binaural source count exceeds the layout limit".to_owned())
        })?;
        for (block, mapping) in blocks.iter_mut().zip(&self.mappings) {
            let channel = frame
                .channels
                .get(mapping.channel_index)
                .and_then(|samples| samples.get(sample_offset..range_end))
                .ok_or_else(|| {
                    OpenJocError::Render("binaural input channel is out of range".to_owned())
                })?;
            *block = BinauralSourceBlock::new(mapping.source_id, channel);
        }
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        let applied_receipt = if let Some(engine) = self.dynamic_engine.as_mut() {
            let before = engine.last_applied_update();
            let logical_count_before = engine.logical_sample_count();
            if self.drain_started {
                engine.render_reconstruction_tail_block(blocks, &mut left, &mut right)?;
            } else {
                engine.render_block(blocks, &mut left, &mut right)?;
            }
            let after = engine.last_applied_update();
            if after == before {
                None
            } else {
                after
                    .map(|receipt| {
                        let relative_start = receipt
                            .logical_start_sample
                            .checked_sub(logical_count_before)
                            .ok_or_else(|| {
                                OpenJocError::Render(
                                    "listener-orientation sample receipt moved backwards"
                                        .to_owned(),
                                )
                            })?;
                        let actual_start = logical_start_sample
                            .checked_add(relative_start)
                            .ok_or_else(|| {
                                OpenJocError::Render(
                                    "listener-orientation sample receipt overflow".to_owned(),
                                )
                            })?;
                        Ok::<AppliedBinauralUpdate, OpenJocError>(AppliedBinauralUpdate {
                            sequence: receipt.sequence,
                            logical_start_sample: actual_start,
                        })
                    })
                    .transpose()?
            }
        } else {
            self.ensure_engine(sample_rate)?
                .render_block(blocks, &mut left, &mut right)?;
            None
        };
        if let Some(receipt) = applied_receipt {
            self.last_orientation_receipt = Some(receipt);
        }
        if self.lfe_policy == BinauralLfePolicy::EqualPowerDualMono {
            if let Some(index) = self.lfe_index {
                if let Some(lfe) = frame
                    .channels
                    .get(index)
                    .and_then(|samples| samples.get(sample_offset..range_end))
                {
                    for ((left_value, right_value), lfe_value) in
                        left.iter_mut().zip(&mut right).zip(lfe)
                    {
                        let lfe_value = self.lfe_delay.process_sample(*lfe_value);
                        *left_value += lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                        *right_value += lfe_value * std::f64::consts::FRAC_1_SQRT_2;
                    }
                }
            }
        }
        Ok(RenderedBlock {
            sample_rate,
            logical_start_sample,
            sample_count,
            channels: vec![left, right],
        })
    }

    pub(super) fn has_no_tail(&self) -> bool {
        let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
            || {
                self.engine
                    .as_ref()
                    .map_or(0, BinauralRenderer::remaining_tail_samples)
            },
            DynamicBinauralRenderer::remaining_tail_samples,
        );
        fir_remaining == 0 && self.lfe_delay.remaining_samples() == 0
    }

    pub(super) fn drain_tail_chunk(
        &mut self,
        max_samples: usize,
        logical_start_sample: u64,
    ) -> Result<Option<RenderedBlock>, OpenJocError> {
        let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
            || {
                self.engine
                    .as_ref()
                    .map_or(0, BinauralRenderer::remaining_tail_samples)
            },
            DynamicBinauralRenderer::remaining_tail_samples,
        );
        let lfe_remaining = self.lfe_delay.remaining_samples();
        let remaining = fir_remaining.max(lfe_remaining);
        if remaining == 0 {
            return Ok(None);
        }
        let sample_count = remaining.min(max_samples);
        let mut left = vec![0.0; sample_count];
        let mut right = vec![0.0; sample_count];
        let fir_count = fir_remaining.min(sample_count);
        if fir_count > 0 {
            if let Some(engine) = self.dynamic_engine.as_mut() {
                engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
            } else if let Some(engine) = self.engine.as_mut() {
                engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
            }
        }
        for (left_value, right_value) in left
            .iter_mut()
            .zip(&mut right)
            .take(lfe_remaining.min(sample_count))
        {
            let lfe = self.lfe_delay.drain_sample() * std::f64::consts::FRAC_1_SQRT_2;
            *left_value += lfe;
            *right_value += lfe;
        }
        Ok(Some(RenderedBlock {
            sample_rate: self.sample_rate_hz,
            logical_start_sample,
            sample_count,
            channels: vec![left, right],
        }))
    }

    pub(super) fn drain_tail(
        &mut self,
        sample_rate: u32,
        start: u64,
    ) -> Result<Vec<RenderedBlock>, OpenJocError> {
        if self.engine.is_none() && self.dynamic_engine.is_none() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        let mut cursor = start;
        loop {
            let fir_remaining = self.dynamic_engine.as_ref().map_or_else(
                || {
                    self.engine
                        .as_ref()
                        .map_or(0, BinauralRenderer::remaining_tail_samples)
                },
                DynamicBinauralRenderer::remaining_tail_samples,
            );
            let remaining = fir_remaining.max(self.lfe_delay.remaining_samples());
            if remaining == 0 {
                break;
            }
            let count = remaining.min(1024);
            let mut left = vec![0.0; count];
            let mut right = vec![0.0; count];
            let fir_count = fir_remaining.min(count);
            if fir_count > 0 {
                if let Some(engine) = self.dynamic_engine.as_mut() {
                    engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
                } else if let Some(engine) = self.engine.as_mut() {
                    engine.drain_tail_block(&mut left[..fir_count], &mut right[..fir_count])?;
                }
            }
            for (left, right) in left
                .iter_mut()
                .zip(&mut right)
                .take(self.lfe_delay.remaining_samples())
            {
                let lfe = self.lfe_delay.drain_sample() * std::f64::consts::FRAC_1_SQRT_2;
                *left += lfe;
                *right += lfe;
            }
            output.push(RenderedBlock {
                sample_rate,
                logical_start_sample: cursor,
                sample_count: count,
                channels: vec![left, right],
            });
            cursor = cursor.saturating_add(count as u64);
        }
        Ok(output)
    }

    pub(super) fn reset(&mut self) -> Result<(), OpenJocError> {
        self.lfe_delay.reset();
        if let Some(engine) = self.dynamic_engine.as_mut() {
            let preparer = self.orientation_preparer.clone().ok_or_else(|| {
                OpenJocError::Render("listener-orientation preparer is unavailable".to_owned())
            })?;
            let next_epoch = engine
                .stream_epoch()
                .checked_add(1)
                .unwrap_or_else(|| engine.stream_epoch());
            let update = preparer
                .prepare(ListenerOrientation::IDENTITY, next_epoch, 0)
                .map_err(|error| OpenJocError::Render(error.to_string()))?;
            let retired = engine
                .reset_to(update)
                .map_err(|failure| OpenJocError::Render(failure.to_string()))?;
            drop(retired);
            self.last_orientation_receipt = None;
        }
        self.drain_started = false;
        if let Some(engine) = self.engine.as_mut() {
            engine.reset();
        }
        Ok(())
    }
}
