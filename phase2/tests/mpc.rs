use phase1::{
    helpers::testing::{setup_verify, CheckForCorrectness},
    parameters::Phase1Parameters,
    Phase1, ProvingSystem,
};
use phase2::{helpers::testing::TestCircuit, parameters::MPCParameters};
use rand::{thread_rng, Rng};
use setup_utils::{BatchExpMode, Groth16Params, UseCompression};
use zexe_algebra::{Bls12_377, Bls12_381, PairingEngine, PrimeField, BW6_761};
use zexe_groth16::{create_random_proof, prepare_verifying_key, verify_proof, Parameters};
use zexe_r1cs_core::{ConstraintSynthesizer, ConstraintSystem, SynthesisMode};

fn generate_mpc_parameters<E, C>(c: C, rng: &mut impl Rng) -> MPCParameters<E>
where
    E: PairingEngine,
    C: Clone + ConstraintSynthesizer<E::Fr>,
{
    let powers = 6; // powers of tau
    let batch = 4;
    let params = Phase1Parameters::<E>::new_full(ProvingSystem::Groth16, powers, batch);
    let compressed = UseCompression::Yes;
    // make 1 power of tau contribution (assume powers of tau gets calculated properly)
    let (_, output, _, _) = setup_verify(
        compressed,
        CheckForCorrectness::Full,
        compressed,
        BatchExpMode::Auto,
        &params,
    );
    let accumulator = Phase1::deserialize(&output, compressed, CheckForCorrectness::Full, &params).unwrap();

    // prepare only the first 32 powers (for whatever reason)
    let groth_params = Groth16Params::<E>::new(
        32,
        accumulator.tau_powers_g1,
        accumulator.tau_powers_g2,
        accumulator.alpha_tau_powers_g1,
        accumulator.beta_tau_powers_g1,
        accumulator.beta_g2,
    )
    .unwrap();
    // write the transcript to a file
    let mut writer = vec![];
    groth_params.write(&mut writer, compressed).unwrap();

    // perform the MPC on only the amount of constraints required for the circuit
    let counter = ConstraintSystem::new_ref();
    counter.set_mode(SynthesisMode::Setup);
    c.clone().generate_constraints(counter.clone()).unwrap();
    let phase2_size = std::cmp::max(
        counter.num_constraints(),
        counter.num_witness_variables() + counter.num_instance_variables() + 1,
    );

    let mut mpc = MPCParameters::<E>::new_from_buffer(
        c,
        writer.as_mut(),
        compressed,
        CheckForCorrectness::Full,
        32,
        phase2_size,
    )
    .unwrap();

    let before = mpc.clone();
    // it is _not_ safe to use it yet, there must be 1 contribution
    mpc.contribute(rng).unwrap();

    before.verify(&mpc).unwrap();

    mpc
}

#[test]
fn test_groth_bls12_377() {
    groth_test_curve::<Bls12_377>()
}

#[test]
fn test_groth_bls12_381() {
    groth_test_curve::<Bls12_381>()
}

#[test]
fn test_groth_bw6() {
    groth_test_curve::<BW6_761>()
}

fn groth_test_curve<E: PairingEngine>() {
    let rng = &mut thread_rng();
    // generate the params
    let params: Parameters<E> = {
        let c = TestCircuit::<E>(None);
        let setup = generate_mpc_parameters(c, rng);
        setup.get_params().clone()
    };

    // Prepare the verification key (for proof verification)
    let pvk = prepare_verifying_key(&params.vk);

    // Create a proof with these params
    let proof = {
        let c = TestCircuit::<E>(Some(<E::Fr as PrimeField>::BigInt::from(5).into()));
        create_random_proof(c, &params, rng).unwrap()
    };

    let res = verify_proof(&pvk, &proof, &[<E::Fr as PrimeField>::BigInt::from(25).into()]);
    assert!(res.is_ok());
}
