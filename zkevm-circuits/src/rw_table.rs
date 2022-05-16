#![allow(missing_docs)]
use eth_types::{Field, ToLittleEndian, ToScalar};
use halo2_proofs::{
    circuit::Region,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, VirtualCells},
    poly::Rotation,
};

use crate::evm_circuit::{table::LookupTable, util::RandomLinearCombination, witness::{RwRow, Rw, RwMap}};

/// The rw table shared between evm circuit and state circuit
#[derive(Clone, Copy)]
pub struct RwTable {
    pub rw_counter: Column<Advice>,
    pub is_write: Column<Advice>,
    pub tag: Column<Advice>,
    pub id: Column<Advice>,
    pub address: Column<Advice>,
    pub field_tag: Column<Advice>,
    pub storage_key: Column<Advice>,
    pub value: Column<Advice>,
    pub value_prev: Column<Advice>,
    pub aux1: Column<Advice>,
    pub aux2: Column<Advice>,
}

impl<F: Field> LookupTable<F> for RwTable {
    fn table_exprs(&self, meta: &mut VirtualCells<F>) -> Vec<Expression<F>> {
        vec![
            meta.query_advice(self.rw_counter, Rotation::cur()),
            meta.query_advice(self.is_write, Rotation::cur()),
            meta.query_advice(self.tag, Rotation::cur()),
            meta.query_advice(self.id, Rotation::cur()),
            meta.query_advice(self.address, Rotation::cur()),
            meta.query_advice(self.field_tag, Rotation::cur()),
            meta.query_advice(self.storage_key, Rotation::cur()),
            meta.query_advice(self.value, Rotation::cur()),
            meta.query_advice(self.value_prev, Rotation::cur()),
            meta.query_advice(self.aux1, Rotation::cur()),
            meta.query_advice(self.aux2, Rotation::cur()),
        ]
    }
}
impl RwTable {
    pub fn construct<F: Field>(meta: &mut ConstraintSystem<F>) -> Self {
        Self {
            rw_counter: meta.advice_column(),
            is_write: meta.advice_column(),
            tag: meta.advice_column(),
            id: meta.advice_column(),
            address: meta.advice_column(),
            field_tag: meta.advice_column(),
            storage_key: meta.advice_column(),
            value: meta.advice_column(),
            value_prev: meta.advice_column(),
            aux1: meta.advice_column(),
            aux2: meta.advice_column(),
        }
    }
    
    pub fn assign<F: Field>(
        &self,
        region: &mut Region<'_, F>,
        randomness: F,
        rw_map: &RwMap,
    )-> Result<Vec<RwRow<F>>, Error> {
        let rows = rw_map.table_assignments(randomness);
        for (offset, row) in rows.iter().enumerate() {
            self.assign_row(region, offset, randomness, row)?;
        }
        Ok(rows)
    }
    pub fn assign_row<F: Field>(
        &self,
        region: &mut Region<'_, F>,
        offset: usize,
        randomness: F,
        row: &RwRow<F>,
    ) -> Result<(), Error> {
        for (column, value) in [
            (self.rw_counter, F::from(row.rw_counter)),
            (self.is_write, F::from(row.is_write)),
            (self.tag, F::from(row.tag)),
            (self.id, F::from(row.id)),
            (self.address, (row.address.to_scalar().unwrap())),
            (self.field_tag, F::from(row.field_tag)),
            (
                self.storage_key,
                RandomLinearCombination::random_linear_combine(
                    row.storage_key.to_le_bytes(),
                    randomness,
                ),
            ),
            (self.value, row.value),
            (self.value_prev, row.value_prev),
            (self.aux1, row.aux1),
            (self.aux2, row.aux2),
        ] {
            region.assign_advice(|| "assign rw row on rw table", column, offset, || Ok(value))?;
        }
        Ok(())
    }
}
