// pattern: Functional Core

use openjoc_render::SampleDelay;

#[test]
fn delay_preserves_short_stream_tail_and_reset() {
    let mut delay = SampleDelay::new(4);
    assert_eq!(delay.process_sample(1.0), 0.0);
    assert_eq!(delay.process_sample(2.0), 0.0);
    let tail: Vec<_> = (0..4).map(|_| delay.drain_sample()).collect();
    assert_eq!(tail, [0.0, 0.0, 1.0, 2.0]);
    assert_eq!(delay.remaining_samples(), 0);
    delay.process_sample(3.0);
    delay.reset();
    assert_eq!(delay.remaining_samples(), 0);
    assert_eq!(delay.process_sample(4.0), 0.0);
    let tail: Vec<_> = (0..4).map(|_| delay.drain_sample()).collect();
    assert_eq!(tail, [0.0, 0.0, 0.0, 4.0]);
}

#[test]
fn delay_is_block_independent_and_zero_delay_preserves_bits() {
    for block_size in [1, 3, 7, 20] {
        let input = [1.0, -2.0, 3.0, 4.0, 0.0, 6.0, 7.0];
        let mut delay = SampleDelay::new(3);
        let mut output = Vec::new();
        for block in input.chunks(block_size) {
            output.extend(block.iter().map(|&sample| delay.process_sample(sample)));
        }
        while delay.remaining_samples() > 0 {
            output.push(delay.drain_sample());
        }
        assert_eq!(&output[..3], &[0.0; 3]);
        assert_eq!(&output[3..], &input);
    }
    let mut delay = SampleDelay::new(0);
    for value in [-0.0_f64, 0.0, 1.2345, f64::MIN_POSITIVE] {
        assert_eq!(delay.process_sample(value).to_bits(), value.to_bits());
    }
    assert_eq!(delay.remaining_samples(), 0);
}
