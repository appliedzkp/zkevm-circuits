use eth_types::Field;
use gadgets::util::Scalar;
use halo2_proofs::{
    circuit::Region,
    plonk::{Error, VirtualCells},
    poly::Rotation,
};

use crate::mpt_circuit::helpers::{num_nibbles, IsEmptyTreeGadget};
use crate::table::ProofType;
use crate::{
    circuit,
    mpt_circuit::MPTContext,
    mpt_circuit::{
        helpers::{key_memory, parent_memory, KeyData, MPTConstraintBuilder, ParentData},
        param::KEY_LEN_IN_NIBBLES,
    },
    mpt_circuit::{MPTConfig, MPTState},
};
use crate::{
    circuit_tools::cell_manager::Cell,
    mpt_circuit::helpers::{DriftedGadget, ParentDataWitness},
};
use crate::{circuit_tools::gadgets::LtGadget, witness::MptUpdateRow};
use crate::{
    circuit_tools::{constraint_builder::RLCChainable, gadgets::IsEqualGadget},
    mpt_circuit::helpers::{main_memory, MainData},
};

use super::{
    helpers::{Indexable, KeyDataWitness, ListKeyGadget, WrongGadget},
    rlp_gadgets::{RLPItemWitness, RLPValueGadget},
    witness_row::{Node, StorageRowType},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct StorageLeafConfig<F> {
    main_data: MainData<F>,
    key_data: [KeyData<F>; 2],
    parent_data: [ParentData<F>; 2],
    rlp_key: [ListKeyGadget<F>; 2],
    value_rlp_bytes: [[Cell<F>; 1]; 2],
    rlp_value: [RLPValueGadget<F>; 2],
    is_wrong_leaf: Cell<F>,
    is_not_hashed: [LtGadget<F, 1>; 2],
    is_in_empty_trie: [IsEmptyTreeGadget<F>; 2],
    drifted: DriftedGadget<F>,
    wrong: WrongGadget<F>,
    is_storage_mod_proof: IsEqualGadget<F>,
    is_non_existing_storage_proof: IsEqualGadget<F>,
}

impl<F: Field> StorageLeafConfig<F> {
    pub fn configure(
        meta: &mut VirtualCells<'_, F>,
        cb: &mut MPTConstraintBuilder<F>,
        ctx: MPTContext<F>,
    ) -> Self {
        let r = ctx.r.clone();

        cb.base
            .cell_manager
            .as_mut()
            .unwrap()
            .reset(StorageRowType::Count as usize);
        let mut config = StorageLeafConfig::default();

        circuit!([meta, cb.base], {
            let key_items = [
                ctx.rlp_item(meta, &mut cb.base, StorageRowType::KeyS as usize),
                ctx.rlp_item(meta, &mut cb.base, StorageRowType::KeyC as usize),
            ];
            config.value_rlp_bytes = [cb.base.query_bytes(), cb.base.query_bytes()];
            let value_item = [
                ctx.rlp_item(meta, &mut cb.base, StorageRowType::ValueS as usize),
                ctx.rlp_item(meta, &mut cb.base, StorageRowType::ValueC as usize),
            ];
            let drifted_item = ctx.rlp_item(meta, &mut cb.base, StorageRowType::Drifted as usize);
            let wrong_item = ctx.rlp_item(meta, &mut cb.base, StorageRowType::Wrong as usize);

            config.main_data = MainData::load(
                "main storage",
                &mut cb.base,
                &ctx.memory[main_memory()],
                0.expr(),
            );

            // Storage leaves always need to be below accounts
            require!(config.main_data.is_below_account => true);

            let mut key_rlc = vec![0.expr(); 2];
            let mut value_rlc = vec![0.expr(); 2];
            let mut value_rlp_rlc = vec![0.expr(); 2];
            for is_s in [true, false] {
                // Parent data
                let parent_data = &mut config.parent_data[is_s.idx()];
                *parent_data = ParentData::load(
                    "leaf load",
                    &mut cb.base,
                    &ctx.memory[parent_memory(is_s)],
                    0.expr(),
                );
                // Key data
                let key_data = &mut config.key_data[is_s.idx()];
                *key_data = KeyData::load(&mut cb.base, &ctx.memory[key_memory(is_s)], 0.expr());

                // Placeholder leaf checks
                config.is_in_empty_trie[is_s.idx()] =
                    IsEmptyTreeGadget::construct(&mut cb.base, parent_data.rlc.expr(), &r);
                let is_placeholder_leaf = config.is_in_empty_trie[is_s.idx()].expr();

                let rlp_key = &mut config.rlp_key[is_s.idx()];
                *rlp_key = ListKeyGadget::construct(&mut cb.base, &key_items[is_s.idx()]);
                config.rlp_value[is_s.idx()] = RLPValueGadget::construct(
                    &mut cb.base,
                    &config.value_rlp_bytes[is_s.idx()]
                        .iter()
                        .map(|c| c.expr())
                        .collect::<Vec<_>>(),
                );

                // Multiplier after list and key
                let mult = rlp_key.rlp_list.rlp_mult(&r) * key_items[is_s.idx()].mult();

                // Because the storage value is an rlp encoded string inside another rlp encoded
                // string (leaves are always encoded as [key, value], with
                // `value` here containing a single stored value) the stored
                // value is either stored directly in the RLP encoded string if short, or stored
                // wrapped inside another RLP encoded string if long.
                (value_rlc[is_s.idx()], value_rlp_rlc[is_s.idx()]) = ifx! {config.rlp_value[is_s.idx()].is_short() => {
                    let value_rlc = value_item[is_s.idx()].rlc_content();
                    (value_rlc.clone(), value_rlc)
                } elsex {
                    let value_rlc = value_item[is_s.idx()].rlc_content();
                    let value_rlp_rlc = (config.rlp_value[is_s.idx()].rlc_rlp(&r), r[0].clone()).rlc_chain(
                        value_item[is_s.idx()].rlc_rlp()
                    );
                    require!(config.rlp_value[is_s.idx()].num_bytes() => value_item[is_s.idx()].num_bytes() + 1.expr());
                    (value_rlc, value_rlp_rlc)
                }};

                let leaf_rlc =
                    (rlp_key.rlc(&r), mult.expr()).rlc_chain(value_rlp_rlc[is_s.idx()].expr());

                // Key
                key_rlc[is_s.idx()] = key_data.rlc.expr()
                    + rlp_key.key.expr(
                        &mut cb.base,
                        rlp_key.key_value.clone(),
                        key_data.mult.expr(),
                        key_data.is_odd.expr(),
                        &r,
                    );
                // Total number of nibbles needs to be KEY_LEN_IN_NIBBLES
                let num_nibbles =
                    num_nibbles::expr(rlp_key.key_value.len(), key_data.is_odd.expr());
                require!(key_data.num_nibbles.expr() + num_nibbles => KEY_LEN_IN_NIBBLES);

                // Placeholder leaves default to value `0`.
                ifx! {is_placeholder_leaf => {
                    require!(value_rlc[is_s.idx()] => 0);
                }}

                // Make sure the RLP encoding is correct.
                // storage = [key, "value"]
                require!(rlp_key.rlp_list.len() => key_items[is_s.idx()].num_bytes() + config.rlp_value[is_s.idx()].num_bytes());

                // Check if the account is in its parent.
                // Check is skipped for placeholder leaves which are dummy leaves
                ifx! {not!(is_placeholder_leaf) => {
                    config.is_not_hashed[is_s.idx()] = LtGadget::construct(&mut cb.base, rlp_key.rlp_list.num_bytes(), 32.expr());
                    ifx!{or::expr(&[parent_data.is_root.expr(), not!(config.is_not_hashed[is_s.idx()])]) => {
                        // Hashed branch hash in parent branch
                        require!((1, leaf_rlc, rlp_key.rlp_list.num_bytes(), parent_data.rlc) => @"keccak");
                    } elsex {
                        // Non-hashed branch hash in parent branch
                        require!(leaf_rlc => parent_data.rlc);
                    }}
                }}

                // Key done, set the default values
                KeyData::store_defaults(&mut cb.base, &ctx.memory[key_memory(is_s)]);
                // Store the new parent
                ParentData::store(
                    &mut cb.base,
                    &ctx.memory[parent_memory(is_s)],
                    0.expr(),
                    true.expr(),
                    false.expr(),
                    0.expr(),
                );
            }

            // Proof types
            config.is_storage_mod_proof = IsEqualGadget::construct(
                &mut cb.base,
                config.main_data.proof_type.expr(),
                ProofType::StorageChanged.expr(),
            );
            config.is_non_existing_storage_proof = IsEqualGadget::construct(
                &mut cb.base,
                config.main_data.proof_type.expr(),
                ProofType::StorageDoesNotExist.expr(),
            );

            // Drifted leaf handling
            config.drifted = DriftedGadget::construct(
                cb,
                &config.parent_data,
                &config.key_data,
                &key_rlc,
                &value_rlp_rlc,
                &drifted_item,
                &ctx.r,
            );

            // Wrong leaf handling
            config.wrong = WrongGadget::construct(
                cb,
                a!(ctx.mpt_table.key_rlc),
                config.is_non_existing_storage_proof.expr(),
                &config.rlp_key[true.idx()].key_value,
                &key_rlc[true.idx()],
                &wrong_item,
                config.is_in_empty_trie[true.idx()].expr(),
                config.key_data[true.idx()].clone(),
                &ctx.r,
            );

            // For non-existing proofs the tree needs to remain the same
            ifx! {config.is_non_existing_storage_proof => {
                require!(config.main_data.root => config.main_data.root_prev);
                require!(key_rlc[true.idx()] => key_rlc[false.idx()]);
            }}

            // Put the data in the lookup table
            let proof_type = matchx! {
                config.is_storage_mod_proof => ProofType::StorageChanged.expr(),
                config.is_non_existing_storage_proof => ProofType::StorageDoesNotExist.expr(),
                _ => ProofType::Disabled.expr(),
            };
            let key_rlc = ifx! {config.is_non_existing_storage_proof => {
                a!(ctx.mpt_table.key_rlc)
            } elsex {
                key_rlc[false.idx()].expr()
            }};
            ctx.mpt_table.constrain(
                meta,
                &mut cb.base,
                config.main_data.address_rlc.expr(),
                proof_type,
                key_rlc,
                value_rlc[true.idx()].expr(),
                value_rlc[false.idx()].expr(),
                config.main_data.root_prev.expr(),
                config.main_data.root.expr(),
            );
        });

        config
    }

    pub fn assign(
        &self,
        region: &mut Region<'_, F>,
        ctx: &MPTConfig<F>,
        pv: &mut MPTState<F>,
        offset: usize,
        node: &Node,
        rlp_values: &[RLPItemWitness],
    ) -> Result<(), Error> {
        let storage = &node.storage.clone().unwrap();

        let key_items = [
            rlp_values[StorageRowType::KeyS as usize].clone(),
            rlp_values[StorageRowType::KeyC as usize].clone(),
        ];
        let value_item = [
            rlp_values[StorageRowType::ValueS as usize].clone(),
            rlp_values[StorageRowType::ValueC as usize].clone(),
        ];
        let drifted_item = rlp_values[StorageRowType::Drifted as usize].clone();
        let wrong_item = rlp_values[StorageRowType::Wrong as usize].clone();

        let main_data =
            self.main_data
                .witness_load(region, offset, &pv.memory[main_memory()], 0)?;

        let mut key_data = vec![KeyDataWitness::default(); 2];
        let mut parent_data = vec![ParentDataWitness::default(); 2];
        let mut key_rlc = vec![0.scalar(); 2];
        let mut value_rlc = vec![0.scalar(); 2];
        for is_s in [true, false] {
            parent_data[is_s.idx()] = self.parent_data[is_s.idx()].witness_load(
                region,
                offset,
                &mut pv.memory[parent_memory(is_s)],
                0,
            )?;

            let rlp_key_witness = self.rlp_key[is_s.idx()].assign(
                region,
                offset,
                &storage.list_rlp_bytes[is_s.idx()],
                &key_items[is_s.idx()],
            )?;

            self.is_not_hashed[is_s.idx()].assign(
                region,
                offset,
                rlp_key_witness.rlp_list.num_bytes().scalar(),
                32.scalar(),
            )?;

            key_data[is_s.idx()] = self.key_data[is_s.idx()].witness_load(
                region,
                offset,
                &mut pv.memory[key_memory(is_s)],
                0,
            )?;
            KeyData::witness_store(
                region,
                offset,
                &mut pv.memory[key_memory(is_s)],
                F::zero(),
                F::one(),
                0,
                F::zero(),
                F::one(),
                0,
            )?;

            // Key
            (key_rlc[is_s.idx()], _) = rlp_key_witness.key.key(
                rlp_key_witness.key_item.clone(),
                key_data[is_s.idx()].rlc,
                key_data[is_s.idx()].mult,
                ctx.r,
            );

            // Value
            for (cell, byte) in self.value_rlp_bytes[is_s.idx()]
                .iter()
                .zip(storage.value_rlp_bytes[is_s.idx()].iter())
            {
                cell.assign(region, offset, byte.scalar())?;
            }
            let value_witness = self.rlp_value[is_s.idx()].assign(
                region,
                offset,
                &storage.value_rlp_bytes[is_s.idx()],
            )?;
            value_rlc[is_s.idx()] = value_item[is_s.idx()].rlc_content(ctx.r);
            
            ParentData::witness_store(
                region,
                offset,
                &mut pv.memory[parent_memory(is_s)],
                F::zero(),
                true,
                false,
                F::zero(),
            )?;

            self.is_in_empty_trie[is_s.idx()].assign(
                region,
                offset,
                parent_data[is_s.idx()].rlc,
                ctx.r,
            )?;
        }

        let is_storage_mod_proof = self.is_storage_mod_proof.assign(
            region,
            offset,
            main_data.proof_type.scalar(),
            ProofType::StorageChanged.scalar(),
        )? == true.scalar();
        let is_non_existing_proof = self.is_non_existing_storage_proof.assign(
            region,
            offset,
            main_data.proof_type.scalar(),
            ProofType::StorageDoesNotExist.scalar(),
        )? == true.scalar();

        // Drifted leaf handling
        self.drifted.assign(
            region,
            offset,
            &parent_data,
            &storage.drifted_rlp_bytes,
            &drifted_item,
            ctx.r,
        )?;

        // Wrong leaf handling
        let key_rlc = self.wrong.assign(
            region,
            offset,
            is_non_existing_proof,
            &key_rlc,
            &storage.wrong_rlp_bytes,
            &wrong_item,
            false,
            key_data[true.idx()].clone(),
            ctx.r,
        )?;

        // Put the data in the lookup table
        let proof_type = if is_storage_mod_proof {
            ProofType::StorageChanged
        } else if is_non_existing_proof {
            ProofType::StorageDoesNotExist
        } else {
            ProofType::Disabled
        };
        ctx.mpt_table.assign(
            region,
            offset,
            &MptUpdateRow {
                address_rlc: main_data.address_rlc,
                proof_type: proof_type.scalar(),
                key_rlc: key_rlc,
                value_prev: value_rlc[true.idx()],
                value: value_rlc[false.idx()],
                root_prev: main_data.root_prev,
                root: main_data.root,
            },
        )?;

        Ok(())
    }
}
