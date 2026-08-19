use super::{
    AV_NOPTS_VALUE, BridgeError, BridgeErrorKind, FfmpegDecoder, FfmpegFrame, Rational,
    ReceiveOutcome,
};
use std::{
    ffi::{CStr, CString, c_char, c_float, c_int, c_uchar, c_uint, c_void},
    marker::PhantomData,
    ptr::NonNull,
    slice,
    time::Instant,
};

const ERROR_CAPACITY: usize = 256;

#[repr(C)]
struct DemuxOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct AvFrameOpaque {
    _private: [u8; 0],
}

#[repr(C)]
struct PacketView {
    data: *const c_uchar,
    size: usize,
    pts: i64,
    dts: i64,
    duration: i64,
    stream_index: c_int,
}

unsafe extern "C" {
    fn openjoc_avutil_version() -> c_uint;
    fn openjoc_avcodec_version() -> c_uint;
    fn openjoc_avformat_version() -> c_uint;

    fn openjoc_av_demux_open(
        path: *const c_char,
        error: *mut c_char,
        error_capacity: usize,
    ) -> *mut DemuxOpaque;
    fn openjoc_av_demux_free(demux: *mut *mut DemuxOpaque);
    fn openjoc_av_demux_find_eac3(demux: *const DemuxOpaque) -> c_int;
    fn openjoc_av_demux_time_base(
        demux: *const DemuxOpaque,
        stream_index: c_int,
        numerator: *mut c_int,
        denominator: *mut c_int,
    ) -> c_int;
    fn openjoc_av_demux_read(
        demux: *mut DemuxOpaque,
        view: *mut PacketView,
        error: *mut c_char,
        error_capacity: usize,
    ) -> c_int;
    fn openjoc_av_demux_seek(
        demux: *mut DemuxOpaque,
        stream_index: c_int,
        timestamp: i64,
        error: *mut c_char,
        error_capacity: usize,
    ) -> c_int;

    fn openjoc_av_channel_id(name: *const c_char) -> c_int;
    fn openjoc_av_frame_create(
        samples: *const c_float,
        sample_len: usize,
        sample_rate: c_int,
        nb_samples: c_int,
        pts: i64,
        has_pts: c_int,
        channel_ids: *const c_int,
        channel_count: c_int,
        standard_layout: *const c_char,
        error: *mut c_char,
        error_capacity: usize,
    ) -> *mut AvFrameOpaque;
    fn openjoc_av_frame_free(frame: *mut *mut AvFrameOpaque);
    fn openjoc_av_frame_data(frame: *const AvFrameOpaque, sample_len: *mut usize)
    -> *const c_float;
    fn openjoc_av_frame_sample_rate(frame: *const AvFrameOpaque) -> c_int;
    fn openjoc_av_frame_nb_samples(frame: *const AvFrameOpaque) -> c_int;
    fn openjoc_av_frame_channel_count(frame: *const AvFrameOpaque) -> c_int;
    fn openjoc_av_frame_pts(frame: *const AvFrameOpaque) -> i64;
    fn openjoc_av_frame_duration(frame: *const AvFrameOpaque) -> i64;
    fn openjoc_av_frame_format(frame: *const AvFrameOpaque) -> c_int;
    fn openjoc_av_sample_format_flt() -> c_int;
    fn openjoc_av_frame_channel(frame: *const AvFrameOpaque, index: c_uint) -> c_int;
    fn openjoc_av_frame_layout_description(
        frame: *const AvFrameOpaque,
        buffer: *mut c_char,
        capacity: usize,
    ) -> c_int;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryVersion {
    pub major: u8,
    pub minor: u8,
    pub micro: u8,
}

impl LibraryVersion {
    fn from_packed(value: u32) -> Self {
        Self {
            major: ((value >> 16) & 0xff) as u8,
            minor: ((value >> 8) & 0xff) as u8,
            micro: (value & 0xff) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegLibraryVersions {
    pub avutil: LibraryVersion,
    pub avcodec: LibraryVersion,
    pub avformat: LibraryVersion,
}

impl FfmpegLibraryVersions {
    #[must_use]
    pub fn current() -> Self {
        // SAFETY: These version functions take no arguments and return values.
        unsafe {
            Self {
                avutil: LibraryVersion::from_packed(openjoc_avutil_version()),
                avcodec: LibraryVersion::from_packed(openjoc_avcodec_version()),
                avformat: LibraryVersion::from_packed(openjoc_avformat_version()),
            }
        }
    }
}

pub struct Demuxer {
    raw: NonNull<DemuxOpaque>,
    target_stream: i32,
    time_base: Rational,
}

impl std::fmt::Debug for Demuxer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Demuxer")
            .field("target_stream", &self.target_stream)
            .field("time_base", &self.time_base)
            .finish_non_exhaustive()
    }
}

impl Demuxer {
    pub fn open(path: &str) -> Result<Self, BridgeError> {
        let path = CString::new(path).map_err(|_| {
            BridgeError::new(
                BridgeErrorKind::InvalidData,
                "input path contains a NUL byte",
            )
        })?;
        let mut error = [0_i8; ERROR_CAPACITY];
        // SAFETY: The path and writable error buffer are valid for this call.
        let raw = unsafe { openjoc_av_demux_open(path.as_ptr(), error.as_mut_ptr(), error.len()) };
        let raw = NonNull::new(raw).ok_or_else(|| ffmpeg_error(&error, "open input failed"))?;
        // SAFETY: `raw` is a live demux object returned above.
        let target_stream = unsafe { openjoc_av_demux_find_eac3(raw.as_ptr()) };
        if target_stream < 0 {
            let mut pointer = raw.as_ptr();
            // SAFETY: Ownership is released exactly once on this error path.
            unsafe { openjoc_av_demux_free(std::ptr::addr_of_mut!(pointer)) };
            return Err(BridgeError::new(
                BridgeErrorKind::Unsupported,
                "libavformat found no E-AC-3 audio stream",
            ));
        }
        let mut numerator = 0;
        let mut denominator = 0;
        // SAFETY: Output pointers and stream index are valid.
        let result = unsafe {
            openjoc_av_demux_time_base(
                raw.as_ptr(),
                target_stream,
                std::ptr::addr_of_mut!(numerator),
                std::ptr::addr_of_mut!(denominator),
            )
        };
        if result < 0 {
            let mut pointer = raw.as_ptr();
            // SAFETY: Ownership is released exactly once on this error path.
            unsafe { openjoc_av_demux_free(std::ptr::addr_of_mut!(pointer)) };
            return Err(BridgeError::new(
                BridgeErrorKind::Ffmpeg,
                "could not read AVStream.time_base",
            ));
        }
        let time_base = Rational::new(numerator, denominator);
        time_base.validate()?;
        Ok(Self {
            raw,
            target_stream,
            time_base,
        })
    }

    #[must_use]
    pub const fn target_stream_index(&self) -> i32 {
        self.target_stream
    }

    #[must_use]
    pub const fn time_base(&self) -> Rational {
        self.time_base
    }

    pub fn read_packet(&mut self) -> Result<Option<DemuxPacket<'_>>, BridgeError> {
        let mut view = PacketView {
            data: std::ptr::null(),
            size: 0,
            pts: AV_NOPTS_VALUE,
            dts: AV_NOPTS_VALUE,
            duration: 0,
            stream_index: -1,
        };
        let mut error = [0_i8; ERROR_CAPACITY];
        // SAFETY: `raw`, `view`, and the error buffer remain valid for the call.
        let result = unsafe {
            openjoc_av_demux_read(
                self.raw.as_ptr(),
                std::ptr::addr_of_mut!(view),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if result == 0 {
            return Ok(None);
        }
        if result < 0 {
            return Err(ffmpeg_error(&error, "av_read_frame failed"));
        }
        let data = if view.size == 0 {
            &[]
        } else {
            if view.data.is_null() {
                return Err(BridgeError::new(
                    BridgeErrorKind::Ffmpeg,
                    "AVPacket has a nonzero size and null data pointer",
                ));
            }
            // SAFETY: The C demux object owns the packet until the next read or drop,
            // and the returned lifetime is tied to this mutable borrow of `self`.
            unsafe { slice::from_raw_parts(view.data, view.size) }
        };
        Ok(Some(DemuxPacket {
            data,
            pts: optional_timestamp(view.pts),
            dts: optional_timestamp(view.dts),
            duration: (view.duration != 0).then_some(view.duration),
            stream_index: view.stream_index,
            time_base: self.time_base,
            _borrow: PhantomData,
        }))
    }

    /// Seeks the selected stream in `AVStream.time_base` units. Call
    /// `FfmpegDecoder::reset` before feeding post-seek packets.
    pub fn seek(&mut self, timestamp: i64) -> Result<(), BridgeError> {
        if timestamp == AV_NOPTS_VALUE {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidTimestamp,
                "cannot seek to AV_NOPTS_VALUE",
            ));
        }
        let mut error = [0_i8; ERROR_CAPACITY];
        // SAFETY: `raw` is live and the error buffer is writable.
        let result = unsafe {
            openjoc_av_demux_seek(
                self.raw.as_ptr(),
                self.target_stream,
                timestamp,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if result < 0 {
            Err(ffmpeg_error(&error, "av_seek_frame failed"))
        } else {
            Ok(())
        }
    }
}

impl Drop for Demuxer {
    fn drop(&mut self) {
        let mut pointer = self.raw.as_ptr();
        // SAFETY: `self` uniquely owns this live demux object.
        unsafe { openjoc_av_demux_free(std::ptr::addr_of_mut!(pointer)) };
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DemuxPacket<'a> {
    pub data: &'a [u8],
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub duration: Option<i64>,
    pub stream_index: i32,
    pub time_base: Rational,
    _borrow: PhantomData<&'a mut Demuxer>,
}

pub struct AvFrame {
    raw: NonNull<AvFrameOpaque>,
}

impl std::fmt::Debug for AvFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AvFrame")
            .field("format", &self.format())
            .field("sample_rate", &self.sample_rate())
            .field("nb_samples", &self.nb_samples())
            .field("channels", &self.channel_count())
            .field("pts", &self.pts())
            .field("duration", &self.duration())
            .finish_non_exhaustive()
    }
}

impl AvFrame {
    pub fn from_frame(frame: &FfmpegFrame) -> Result<Self, BridgeError> {
        let channels = frame
            .channel_layout
            .ffmpeg_order
            .iter()
            .map(|name| {
                let name = CString::new(name.as_str()).map_err(|_| {
                    BridgeError::new(BridgeErrorKind::InvalidConfig, "channel name contains NUL")
                })?;
                // SAFETY: The channel name is a valid C string for this call.
                let id = unsafe { openjoc_av_channel_id(name.as_ptr()) };
                if id < 0 {
                    Err(BridgeError::new(
                        BridgeErrorKind::Unsupported,
                        format!("FFmpeg does not recognize channel {name:?}"),
                    ))
                } else {
                    Ok(id)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let standard = frame
            .channel_layout
            .standard_layout
            .as_deref()
            .map(CString::new)
            .transpose()
            .map_err(|_| {
                BridgeError::new(BridgeErrorKind::InvalidConfig, "layout name contains NUL")
            })?;
        let sample_rate = i32::try_from(frame.sample_rate).map_err(|_| {
            BridgeError::new(BridgeErrorKind::InvalidData, "sample rate exceeds C int")
        })?;
        let nb_samples = i32::try_from(frame.nb_samples).map_err(|_| {
            BridgeError::new(BridgeErrorKind::InvalidData, "sample count exceeds C int")
        })?;
        let channel_count = i32::try_from(channels.len()).map_err(|_| {
            BridgeError::new(BridgeErrorKind::InvalidData, "channel count exceeds C int")
        })?;
        let mut error = [0_i8; ERROR_CAPACITY];
        // SAFETY: All pointers describe live buffers for this call. The C side
        // allocates an owned AVFrame and copies the samples into AVBuffer data.
        let raw = unsafe {
            openjoc_av_frame_create(
                frame.interleaved_f32.as_ptr(),
                frame.interleaved_f32.len(),
                sample_rate,
                nb_samples,
                frame.pts.unwrap_or(AV_NOPTS_VALUE),
                i32::from(frame.pts.is_some()),
                channels.as_ptr(),
                channel_count,
                standard
                    .as_ref()
                    .map_or(std::ptr::null(), |name| name.as_ptr()),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        Ok(Self {
            raw: NonNull::new(raw)
                .ok_or_else(|| ffmpeg_error(&error, "AVFrame allocation failed"))?,
        })
    }

    /// Borrow the actual FFmpeg AVFrame pointer for an embedding application.
    #[must_use]
    pub fn as_ptr(&self) -> *const c_void {
        self.raw.as_ptr().cast()
    }

    #[must_use]
    pub fn format(&self) -> i32 {
        // SAFETY: `raw` is live for `self`'s lifetime.
        unsafe { openjoc_av_frame_format(self.raw.as_ptr()) }
    }

    #[must_use]
    pub fn is_packed_float(&self) -> bool {
        // SAFETY: Both calls are pure reads from live FFmpeg state.
        unsafe { self.format() == openjoc_av_sample_format_flt() }
    }

    #[must_use]
    pub fn sample_rate(&self) -> i32 {
        // SAFETY: `raw` is live for `self`'s lifetime.
        unsafe { openjoc_av_frame_sample_rate(self.raw.as_ptr()) }
    }

    #[must_use]
    pub fn nb_samples(&self) -> i32 {
        // SAFETY: `raw` is live for `self`'s lifetime.
        unsafe { openjoc_av_frame_nb_samples(self.raw.as_ptr()) }
    }

    #[must_use]
    pub fn channel_count(&self) -> i32 {
        // SAFETY: `raw` is live for `self`'s lifetime.
        unsafe { openjoc_av_frame_channel_count(self.raw.as_ptr()) }
    }

    #[must_use]
    pub fn pts(&self) -> Option<i64> {
        // SAFETY: `raw` is live for `self`'s lifetime.
        optional_timestamp(unsafe { openjoc_av_frame_pts(self.raw.as_ptr()) })
    }

    #[must_use]
    pub fn duration(&self) -> i64 {
        // SAFETY: `raw` is live for `self`'s lifetime.
        unsafe { openjoc_av_frame_duration(self.raw.as_ptr()) }
    }

    #[must_use]
    pub fn channel_ids(&self) -> Vec<i32> {
        let count = u32::try_from(self.channel_count()).unwrap_or(0);
        (0..count)
            .map(|index| {
                // SAFETY: Each index is within the reported channel count.
                unsafe { openjoc_av_frame_channel(self.raw.as_ptr(), index) }
            })
            .collect()
    }

    pub fn layout_description(&self) -> Result<String, BridgeError> {
        let mut buffer = [0_i8; ERROR_CAPACITY];
        // SAFETY: `raw` and the writable description buffer are valid.
        let result = unsafe {
            openjoc_av_frame_layout_description(
                self.raw.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };
        if result < 0 {
            return Err(BridgeError::new(
                BridgeErrorKind::Ffmpeg,
                "av_channel_layout_describe failed",
            ));
        }
        Ok(c_buffer(&buffer).unwrap_or_else(|| "unknown layout".to_owned()))
    }

    #[must_use]
    pub fn interleaved_f32(&self) -> &[f32] {
        let mut len = 0;
        // SAFETY: `raw` is live and the returned data is owned by its AVBuffer.
        let data = unsafe { openjoc_av_frame_data(self.raw.as_ptr(), std::ptr::addr_of_mut!(len)) };
        if data.is_null() {
            &[]
        } else {
            // SAFETY: The C helper reports the packed audio allocation length.
            unsafe { slice::from_raw_parts(data, len) }
        }
    }
}

impl Drop for AvFrame {
    fn drop(&mut self) {
        let mut pointer = self.raw.as_ptr();
        // SAFETY: `self` uniquely owns the AVFrame and releases it once.
        unsafe { openjoc_av_frame_free(std::ptr::addr_of_mut!(pointer)) };
    }
}

#[derive(Debug)]
pub enum ReceiveAvOutcome {
    Frame(AvFrame),
    NeedMoreInput,
    EndOfStream,
    NotJoc,
}

impl FfmpegDecoder {
    pub fn receive_avframe(&mut self) -> Result<ReceiveAvOutcome, BridgeError> {
        match self.receive_frame()? {
            ReceiveOutcome::Frame(frame) => {
                let started = Instant::now();
                let frame = AvFrame::from_frame(&frame)?;
                self.timings.avframe_allocation_nanos = self
                    .timings
                    .avframe_allocation_nanos
                    .saturating_add(started.elapsed().as_nanos());
                Ok(ReceiveAvOutcome::Frame(frame))
            }
            ReceiveOutcome::NeedMoreInput => Ok(ReceiveAvOutcome::NeedMoreInput),
            ReceiveOutcome::EndOfStream => Ok(ReceiveAvOutcome::EndOfStream),
            ReceiveOutcome::NotJoc => Ok(ReceiveAvOutcome::NotJoc),
        }
    }
}

fn optional_timestamp(value: i64) -> Option<i64> {
    (value != AV_NOPTS_VALUE).then_some(value)
}

fn ffmpeg_error(buffer: &[c_char], fallback: &str) -> BridgeError {
    BridgeError::new(
        BridgeErrorKind::Ffmpeg,
        c_buffer(buffer).unwrap_or_else(|| fallback.to_owned()),
    )
}

fn c_buffer(buffer: &[c_char]) -> Option<String> {
    if buffer.first().copied().unwrap_or(0) == 0 {
        return None;
    }
    // SAFETY: Every buffer passed here is zero-initialized and C writes are
    // capacity-bounded, so at least the final untouched byte remains NUL.
    let value = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Some(value.to_string_lossy().into_owned())
}
