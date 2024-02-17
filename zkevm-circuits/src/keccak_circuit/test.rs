use super::*;
use crate::{
    evm_circuit::util::rlc,
    util::{unusable_rows, word::WordLoHi},
};
use bus_mapping::state_db::EMPTY_CODE_HASH_LE;
use eth_types::{Field, H256, U256};
use halo2_proofs::{
    dev::{CellValue, MockProver},
    halo2curves::bn256::Fr,
    plonk::{Assignment, Circuit},
};
use itertools::izip;
use log::error;
use std::iter::zip;

use super::util::{target_part_sizes, target_part_sizes_rot, WordParts};

// This needs to be tested independent since it sets the environment variable
// which might affect other tests.
#[ignore]
#[test]
fn serial_keccak_circuit_unusable_rows() {
    for keccak_rows in NUM_BYTES_PER_WORD + 1..=32 {
        std::env::set_var("KECCAK_ROWS", format!("{keccak_rows}"));
        assert_eq!(
            KeccakCircuit::<Fr>::unusable_rows(),
            unusable_rows::<Fr, KeccakCircuit::<Fr>>(()),
        )
    }
    std::env::set_var("KECCAK_ROWS", format!("{DEFAULT_KECCAK_ROWS}"));
}

const EMPTY_DIGEST: &str = "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470";

fn verify<F: Field>(k: u32, inputs: Vec<Vec<u8>>, digests: Vec<String>, success: bool) {
    let circuit = KeccakCircuit::new(2usize.pow(k), inputs.clone());
    let prover = MockProver::<F>::run(k, &circuit, vec![]).unwrap();
    let (config, challenges) = KeccakCircuit::configure(&mut ConstraintSystem::<F>::default());
    let input_challenge = prover.get_challenge(challenges.keccak_input());

    // Check constraints.
    let verify_result = prover.verify();
    if verify_result.is_ok() != success {
        if let Some(errors) = verify_result.err() {
            for error in errors.iter() {
                error!("{}", error);
            }
        }
        panic!();
    }

    // Extract the content of the lookup table.
    let hash_lookup_table = {
        // Find the columns of the table.
        let is_enabled = prover.advice_values(config.keccak_table.is_enabled);
        let input_rlc = prover.advice_values(config.keccak_table.input_rlc);
        let input_len = prover.advice_values(config.keccak_table.input_len);
        let output = config
            .keccak_table
            .output
            .map(|limb| prover.advice_values(limb));

        // Keep the rows that are supposed to contain hash results.
        izip!(is_enabled, input_rlc, input_len, output.lo(), output.hi())
            .filter_map(|(enabled, input_rlc, input_len, output_lo, output_hi)| {
                assigned_non_zero(enabled).then(|| {
                    (
                        unwrap(input_rlc),
                        unwrap(input_len),
                        unwrap(output_lo),
                        unwrap(output_hi),
                    )
                })
            })
            .collect::<Vec<(F, F, F, F)>>()
    };

    // Calculate the expected inputs in reversed-RLC form.
    let rlc_input =
        |bytes: &[u8]| input_challenge.map(|random| rlc::value(bytes.iter().rev(), random));

    // Check that all the digests are there.
    assert!(hash_lookup_table.len() >= inputs.len());
    assert_eq!(inputs.len(), digests.len());
    for (input, digest, hash) in izip!(&inputs, &digests, &hash_lookup_table) {
        let len = F::from(input.len() as u64);
        let digest_slice: [u8; 32] = hex::decode(digest).unwrap().try_into().unwrap();
        let (lo, hi): (F, F) = WordLoHi::from(H256::from(digest_slice)).to_lo_hi();

        let expected = (rlc_input(input), len, lo, hi);

        expected.0.assert_if_known(|value| *value == hash.0);
        assert_eq!(hash.1, expected.1);
        assert_eq!(hash.2, expected.2);
        assert_eq!(hash.3, expected.3);
    }
    let (lo, hi) =
        WordLoHi::from(U256::from_little_endian(EMPTY_CODE_HASH_LE.as_slice())).to_lo_hi();

    // Check that other digests are the digest of the empty message.
    let empty_hash = (F::ZERO, F::ZERO, lo, hi);
    for hash in hash_lookup_table.iter().skip(inputs.len()) {
        assert_eq!(*hash, empty_hash);
    }
}

#[test]
fn packed_multi_keccak_simple() {
    let k = 14;
    let inputs = vec![
        vec![],
        vec![0],
        (0u8..135).collect::<Vec<_>>(),
        (0u8..136).collect::<Vec<_>>(),
        (0u8..137).collect::<Vec<_>>(),
        (0..400).map(|i| (1 + 3 * i) as u8).collect::<Vec<_>>(),
    ];
    let digests = vec![
        EMPTY_DIGEST.to_string(),
        "bc36789e7a1e281436464229828f817d6612f7b477d66591ff96a9e064bcc98a".to_string(),
        "cbdfd9dee5faad3818d6b06f95a219fd290b0e1706f6a82e5a595b9ce9faca62".to_string(),
        "7ce759f1ab7f9ce437719970c26b0a66ff11fe3e38e17df89cf5d29c7d7f807e".to_string(),
        "ac73d4fae68b8453f764007c1a20ce95994187861f0c3227a3a8e99a73a3b1db".to_string(),
        "f46dfb05481d2a50c0c3b6625d913055da3e07dcd0d6c661f27f1449b0fed7eb".to_string(),
    ];
    verify::<Fr>(k, inputs, digests, true);
}

fn assigned_non_zero<F: Field>(cv: &CellValue<F>) -> bool {
    match *cv {
        CellValue::Assigned(v) => !v.is_zero_vartime(),
        _ => false,
    }
}

fn unwrap<F: Field>(cv: &CellValue<F>) -> F {
    match *cv {
        CellValue::Assigned(f) => f,
        _ => panic!("the cell should be assigned"),
    }
}

#[test]
fn variadic_size_check() {
    let k = 14;
    let num_rows = 2usize.pow(k);
    // Empty
    let inputs = vec![];
    let circuit = KeccakCircuit::new(num_rows, inputs);
    let prover1 = MockProver::<Fr>::run(k, &circuit, vec![]).unwrap();

    // Non-empty
    let inputs = vec![
        vec![],
        (0u8..1).collect::<Vec<_>>(),
        (0u8..135).collect::<Vec<_>>(),
        (0u8..136).collect::<Vec<_>>(),
        (0u8..200).collect::<Vec<_>>(),
    ];
    let circuit = KeccakCircuit::new(num_rows, inputs);
    let prover2 = MockProver::<Fr>::run(k, &circuit, vec![]).unwrap();

    assert_eq!(prover1.fixed(), prover2.fixed());
    assert_eq!(prover1.permutation(), prover2.permutation());
}

#[test]
fn test_target_part_sizes() {
    // Uniform 8 parts of 8 bits each.
    assert_eq!(target_part_sizes_rot(8, 0), vec![8, 8, 8, 8, 8, 8, 8, 8]);
    // With rotations.
    assert_eq!(target_part_sizes_rot(8, 1), vec![1, 8, 8, 8, 8, 8, 8, 8, 7]);
    assert_eq!(target_part_sizes_rot(8, 2), vec![2, 8, 8, 8, 8, 8, 8, 8, 6]);
    assert_eq!(target_part_sizes_rot(8, 8), vec![8, 8, 8, 8, 8, 8, 8, 8]);
    assert_eq!(target_part_sizes_rot(8, 9), vec![8, 1, 8, 8, 8, 8, 8, 8, 7]);

    // Parts of 11 bits and the remaining 9 bits.
    assert_eq!(target_part_sizes_rot(11, 0), vec![11, 11, 11, 11, 11, 9]);
    // With rotations.
    assert_eq!(target_part_sizes_rot(11, 1), vec![1, 11, 11, 11, 11, 11, 8]);
    assert_eq!(target_part_sizes_rot(11, 2), vec![2, 11, 11, 11, 11, 11, 7]);
    assert_eq!(target_part_sizes_rot(11, 11), vec![11, 11, 11, 11, 11, 9]);
    assert_eq!(
        target_part_sizes_rot(11, 12),
        vec![11, 1, 11, 11, 11, 11, 8]
    );

    // "uniform" is the same as rot=0
    assert_eq!(target_part_sizes(8), target_part_sizes_rot(8, 0));
    assert_eq!(target_part_sizes(11), target_part_sizes_rot(11, 0));
}

#[test]
fn test_word_parts() {
    {
        let word = WordParts::new(11, 0, true);
        // Parts of bits.
        let expected: Vec<Vec<usize>> = vec![
            (0..11).collect(),  // Length 11
            (11..22).collect(), // …
            (22..33).collect(), // …
            (33..44).collect(), // …
            (44..55).collect(), // …
            (55..64).collect(), // Length 9
        ];
        assert_eq!(word.parts.len(), expected.len());
        for (part, xbits) in zip(word.parts, expected) {
            assert_eq!(part.bits, xbits);
        }
    }

    {
        let word = WordParts::new(11, 1, false);
        // Parts of bits.
        let expected: Vec<Vec<usize>> = vec![
            (0..11).collect(),  // Length 11
            (11..22).collect(), // …
            (22..33).collect(), // …
            (33..44).collect(), // …
            (44..55).collect(), // …
            (55..63).collect(), // Length 8
            (63..64).collect(), // Length 1
        ];
        assert_eq!(word.parts.len(), expected.len());
        for (part, xbits) in zip(word.parts, expected) {
            assert_eq!(part.bits, xbits);
        }
    }
}
