// Functional Core: valid, deterministic in-memory sparse SOFA fixtures.
use hdf5_pure::{AttrValue, FileBuilder};

pub fn spherical_fixture(sources: &[[f64; 3]]) -> Vec<u8> {
    let mut file = FileBuilder::new();
    for (name, value) in [
        ("Conventions", "SOFA"),
        ("SOFAConventions", "SimpleFreeFieldHRIR"),
        ("SOFAConventionsVersion", "1.2"),
        ("DataType", "FIR"),
        ("RoomType", "free field"),
    ] {
        file.set_attr(name, AttrValue::String(value.into()));
    }
    let measurements = sources.len() as u64;
    let taps = (1..=sources.len())
        .flat_map(|index| [index as f64, 0.0, index as f64 * 0.75, 0.0])
        .collect::<Vec<_>>();
    file.create_dataset("Data.IR")
        .with_f64_data(&taps)
        .with_shape(&[measurements, 2, 2]);
    file.create_dataset("Data.SamplingRate")
        .with_f64_data(&[48_000.0])
        .with_shape(&[1])
        .set_attr("Units", AttrValue::String("hertz".into()));
    file.create_dataset("Data.Delay")
        .with_f64_data(&[0.0, 0.0])
        .with_shape(&[1, 2])
        .set_attr("Units", AttrValue::String("samples".into()));
    for (name, data, shape, kind, units) in [
        (
            "ListenerPosition",
            vec![0.0, 0.0, 0.0],
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ListenerView",
            vec![1.0, 0.0, 0.0],
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ListenerUp",
            vec![0.0, 0.0, 1.0],
            vec![1, 3],
            "cartesian",
            "metre",
        ),
        (
            "ReceiverPosition",
            vec![0.0, 0.09, 0.0, 0.0, -0.09, 0.0],
            vec![2, 3, 1],
            "cartesian",
            "metre",
        ),
        (
            "SourcePosition",
            sources.iter().flatten().copied().collect(),
            vec![measurements, 3],
            "spherical",
            "degree, degree, metre",
        ),
    ] {
        file.create_dataset(name)
            .with_f64_data(&data)
            .with_shape(&shape)
            .set_attr("Type", AttrValue::String(kind.into()))
            .set_attr("Units", AttrValue::String(units.into()));
    }
    file.finish().expect("valid sparse HDF5 SOFA fixture")
}

pub fn cap_fixture(elevation: f64, stereo: bool) -> Vec<u8> {
    let mut sources = vec![
        [0.0, elevation, 1.0],
        [120.0, elevation, 1.0],
        [240.0, elevation, 1.0],
    ];
    if stereo {
        sources.extend([[45.0, 0.0, 1.0], [-45.0, 0.0, 1.0]]);
    }
    spherical_fixture(&sources)
}
