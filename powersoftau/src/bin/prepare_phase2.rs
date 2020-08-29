use gumdrop::Options;
use powersoftau::{
    cli_common::{curve_from_str, CurveKind},
    parameters::*,
    BatchedAccumulator,
};
use snark_utils::{CheckForCorrectness, Groth16Params, Result, UseCompression};

use std::time::Instant;
use zexe_algebra::{Bls12_377, Bls12_381, Bn254, PairingEngine, BW6_761};

use std::fs::OpenOptions;

use memmap::*;
use tracing_subscriber::{
    filter::EnvFilter,
    fmt::{time::ChronoUtc, Subscriber},
};

#[derive(Debug, Options, Clone)]
struct PreparePhase2Opts {
    help: bool,
    #[options(
        help = "the file which will contain the FFT coefficients processed for Phase 2 of the setup"
    )]
    phase2_fname: String,
    #[options(
        help = "the response file which will be processed for the specialization (phase 2) of the setup"
    )]
    response_fname: String,
    #[options(
        help = "the elliptic curve to use",
        default = "bls12_377",
        parse(try_from_str = "curve_from_str")
    )]
    pub curve_kind: CurveKind,
    #[options(help = "the size of batches to process", default = "256")]
    pub batch_size: usize,
    #[options(
        help = "the number of powers used for phase 1 (circuit size will be 2^{power})",
        default = "21"
    )]
    pub power: usize,
    #[options(help = "the size (in powers) of the phase 2 circuit", default = "21")]
    pub phase2_size: u32,
}

fn main() -> Result<()> {
    Subscriber::builder()
        .with_timer(ChronoUtc::rfc3339())
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let opts = PreparePhase2Opts::parse_args_default_or_exit();

    let now = Instant::now();
    match opts.curve_kind {
        CurveKind::Bls12_381 => prepare_phase2::<Bls12_381>(&opts)?,
        CurveKind::Bls12_377 => prepare_phase2::<Bls12_377>(&opts)?,
        CurveKind::BW6 => prepare_phase2::<BW6_761>(&opts)?,
        CurveKind::Bn254 => prepare_phase2::<Bn254>(&opts)?,
    }

    let new_now = Instant::now();
    println!(
        "Executing {:?} took: {:?}",
        opts,
        new_now.duration_since(now)
    );

    Ok(())
}

fn prepare_phase2<E: PairingEngine + Sync>(opts: &PreparePhase2Opts) -> Result<()> {
    let parameters = CeremonyParams::<E>::new_for_first_chunk(opts.power, opts.batch_size);
    // Try to load response file from disk.
    let reader = OpenOptions::new()
        .read(true)
        .open(&opts.response_fname)
        .expect("unable open response file in this directory");
    let response_readable_map = unsafe {
        MmapOptions::new()
            .map(&reader)
            .expect("unable to create a memory map for input")
    };

    // Create the parameter file
    let mut writer = OpenOptions::new()
        .read(false)
        .write(true)
        .create_new(true)
        .open(&opts.phase2_fname)
        .expect("unable to create parameter file in this directory");

    // Deserialize the accumulator
    let current_accumulator = BatchedAccumulator::deserialize(
        &response_readable_map,
        UseCompression::Yes,
        CheckForCorrectness::No, // No need to check for correctness since it's been checked by the transcript verifier.
        &parameters,
    )
    .expect("unable to read uncompressed accumulator");

    // Load the elements to the Groth16 utility
    let groth16_params = Groth16Params::<E>::new(
        2usize.pow(opts.phase2_size),
        current_accumulator.tau_powers_g1,
        current_accumulator.tau_powers_g2,
        current_accumulator.alpha_tau_powers_g1,
        current_accumulator.beta_tau_powers_g1,
        current_accumulator.beta_g2,
    )
    .expect("could not create Groth16 Lagrange coefficients");

    // Write the parameters
    groth16_params.write(&mut writer, UseCompression::No)?;

    Ok(())
}
