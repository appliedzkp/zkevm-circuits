use bus_mapping::circuit_input_builder::ChunkContext;
use gadgets::{
    is_zero::{IsZeroChip, IsZeroConfig, IsZeroInstruction},
    util::Expr,
};
use halo2_proofs::{
    circuit::{Layouter, Value},
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Instance, Selector},
    poly::Rotation,
};

use crate::{
    evm_circuit::util::rlc,
    table::{
        chunkctx_table::{ChunkCtxFieldTag, ChunkCtxTable},
        LookupTable,
    },
};
use eth_types::Field;

use super::Challenges;

/// chunk context config
#[derive(Clone, Debug)]
pub struct ChunkContextConfig<F> {
    chunk_index: Column<Advice>,
    chunk_index_next: Column<Advice>,
    total_chunks: Column<Advice>,
    qchunk_context: Selector,

    /// is_first_chunk config
    pub is_first_chunk: IsZeroConfig<F>,
    /// is_lastchunk config
    pub is_lastchunk: IsZeroConfig<F>,

    /// ChunkCtxTable
    pub chunkctx_table: ChunkCtxTable,
    /// instance column for prev chunk context
    pub pi_prechunkctx: Column<Instance>,
    /// instance column for next chunk context
    pub pi_nextchunkctx: Column<Instance>,
}

impl<F: Field> ChunkContextConfig<F> {
    /// new a chunk context config
    pub fn new(meta: &mut ConstraintSystem<F>, challenges: &Challenges<Expression<F>>) -> Self {
        let qchunk_context = meta.complex_selector();
        let chunk_index = meta.advice_column();
        let chunk_index_inv = meta.advice_column();
        let chunk_index_next = meta.advice_column();
        let chunk_diff = meta.advice_column();
        let total_chunks = meta.advice_column();

        let pi_prechunkctx = meta.instance_column();
        let pi_nextchunkctx = meta.instance_column();
        meta.enable_equality(pi_prechunkctx);
        meta.enable_equality(pi_nextchunkctx);

        let chunkctx_table = ChunkCtxTable::construct(meta);
        chunkctx_table.annotate_columns(meta);

        [
            (ChunkCtxFieldTag::CurrentChunkIndex.expr(), chunk_index),
            (ChunkCtxFieldTag::NextChunkIndex.expr(), chunk_index_next),
            (ChunkCtxFieldTag::TotalChunks.expr(), total_chunks),
        ]
        .iter()
        .for_each(|(tag_expr, value_col)| {
            meta.lookup_any("chunk context lookup", |meta| {
                let qchunk_context = meta.query_selector(qchunk_context);
                let value_col_expr = meta.query_advice(*value_col, Rotation::cur());

                vec![(
                    qchunk_context
                        * rlc::expr(
                            &[tag_expr.clone(), value_col_expr],
                            challenges.lookup_input(),
                        ),
                    rlc::expr(&chunkctx_table.table_exprs(meta), challenges.lookup_input()),
                )]
            });
        });

        let is_first_chunk = IsZeroChip::configure(
            meta,
            |meta| meta.query_selector(qchunk_context),
            |meta| meta.query_advice(chunk_index, Rotation::cur()),
            chunk_index_inv,
        );

        let is_lastchunk = IsZeroChip::configure(
            meta,
            |meta| meta.query_selector(qchunk_context),
            |meta| {
                let chunk_index = meta.query_advice(chunk_index, Rotation::cur());
                let total_chunks = meta.query_advice(total_chunks, Rotation::cur());

                total_chunks - chunk_index - 1.expr()
            },
            chunk_diff,
        );

        Self {
            qchunk_context,
            chunk_index,
            chunk_index_next,
            total_chunks,
            is_first_chunk,
            is_lastchunk,
            chunkctx_table,
            pi_prechunkctx,
            pi_nextchunkctx,
        }
    }

    /// assign chunk context
    pub fn assignchunk_context(
        &self,
        layouter: &mut impl Layouter<F>,
        chunk_context: &ChunkContext,
        max_offset_index: usize,
    ) -> Result<(), Error> {
        let (
            chunk_index_cell,
            chunk_index_next_cell,
            totalchunk_cell,
            initial_rwc_cell,
            end_rwc_cell,
        ) = self.chunkctx_table.load(layouter, chunk_context)?;

        let is_first_chunk = IsZeroChip::construct(self.is_first_chunk.clone());
        let is_lastchunk = IsZeroChip::construct(self.is_lastchunk.clone());
        layouter.assign_region(
            || "chunk context",
            |mut region| {
                region.name_column(|| "chunk_index", self.chunk_index);
                region.name_column(|| "chunk_index_next", self.chunk_index_next);
                region.name_column(|| "total_chunks", self.total_chunks);
                region.name_column(|| "pi_prechunkctx", self.pi_prechunkctx);
                region.name_column(|| "pi_nextchunkctx", self.pi_nextchunkctx);
                self.is_first_chunk
                    .annotate_columns_in_region(&mut region, "is_first_chunk");
                self.is_lastchunk
                    .annotate_columns_in_region(&mut region, "is_lastchunk");
                self.chunkctx_table.annotate_columns_in_region(&mut region);

                for offset in 0..max_offset_index + 1 {
                    self.qchunk_context.enable(&mut region, offset)?;

                    region.assign_advice(
                        || "chunk_index",
                        self.chunk_index,
                        offset,
                        || Value::known(F::from(chunk_context.chunk_index as u64)),
                    )?;

                    region.assign_advice(
                        || "chunk_index_next",
                        self.chunk_index_next,
                        offset,
                        || Value::known(F::from(chunk_context.chunk_index as u64 + 1u64)),
                    )?;

                    region.assign_advice(
                        || "total_chunks",
                        self.total_chunks,
                        offset,
                        || Value::known(F::from(chunk_context.total_chunks as u64)),
                    )?;

                    is_first_chunk.assign(
                        &mut region,
                        offset,
                        Value::known(F::from(chunk_context.chunk_index as u64)),
                    )?;
                    is_lastchunk.assign(
                        &mut region,
                        offset,
                        Value::known(F::from(
                            (chunk_context.total_chunks - chunk_context.chunk_index - 1) as u64,
                        )),
                    )?;
                }
                Ok(())
            },
        )?;

        vec![chunk_index_cell, totalchunk_cell.clone(), initial_rwc_cell]
            .iter()
            .enumerate()
            .try_for_each(|(i, cell)| {
                layouter.constrain_instance(cell.cell(), self.pi_prechunkctx, i)
            })?;
        [chunk_index_next_cell, totalchunk_cell, end_rwc_cell]
            .iter()
            .enumerate()
            .try_for_each(|(i, cell)| {
                layouter.constrain_instance(cell.cell(), self.pi_nextchunkctx, i)
            })?;

        Ok(())
    }
}
