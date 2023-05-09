use eth_types::ToLittleEndian;
use gadgets::util::{not, or, Expr};
use halo2_proofs::{
    circuit::{AssignedCell, Value},
    halo2curves::FieldExt,
    plonk::{Error, Expression},
};
use itertools::Itertools;

use crate::evm_circuit::util::{from_bytes, CachedRegion, Cell};

#[derive(Clone, Debug)]
pub(crate) struct WordLimbs<T, const N: usize> {
    pub limbs: [T; N],
}

pub(crate) type Word2<T> = WordLimbs<T, 2>;

pub(crate) type Word4<T> = WordLimbs<T, 4>;

pub(crate) type Word16<T> = WordLimbs<T, 16>;

pub(crate) type Word32<T> = WordLimbs<T, 32>;

pub(crate) type WordCell<F> = Word<Cell<F>>;

pub(crate) type Word32Cell<F> = Word32<Cell<F>>;

impl<T, const N: usize> WordLimbs<T, N> {
    pub fn new(limbs: [T; N]) -> Self {
        Self { limbs }
    }

    pub fn n() -> usize {
        N
    }
}

impl<T: Default, const N: usize> Default for WordLimbs<T, N> {
    fn default() -> Self {
        Self {
            limbs: [(); N].map(|_| T::default()),
        }
    }
}

impl<F: FieldExt, const N: usize> WordLimbs<Cell<F>, N> {
    pub fn assign<const N1: usize>(
        &self,
        region: &mut CachedRegion<'_, '_, F>,
        offset: usize,
        bytes: Option<[u8; N1]>,
    ) -> Result<Vec<AssignedCell<F, F>>, Error> {
        assert_eq!(N1 % N, 0); // TODO assure N|N1, find way to use static_assertion instead
        bytes.map_or(Err(Error::Synthesis), |bytes| {
            bytes
                .chunks(N1 / N) // chunk in little endian
                .map(|chunk| from_bytes::value(chunk))
                .zip(self.limbs.iter())
                .map(|(value, cell)| cell.assign(region, offset, Value::known(value)))
                .collect()
        })
    }

    pub fn expr(&self) -> WordLimbs<Expression<F>, N> {
        return WordLimbs::new(self.limbs.map(|cell| cell.expr()));
    }

    pub fn to_word(&self) -> Word<Expression<F>> {
        Word(self.expr().to_wordlimbs())
    }
}

// `Word`, special alias for Word2.
#[derive(Clone, Debug)]
pub(crate) struct Word<T>(Word2<T>);

impl<T> Word<T> {
    pub fn new(limbs: [T; 2]) -> Self {
        Self(WordLimbs::<T, 2>::new(limbs))
    }
    pub fn hi(&self) -> &T {
        &self.0.limbs[1]
    }
    pub fn lo(&self) -> &T {
        &self.0.limbs[0]
    }

    pub fn n() -> usize {
        2
    }

    pub fn to_lo_hi(&self) -> (&T, &T) {
        (&self.0.limbs[0], &self.0.limbs[1])
    }
}

impl<T> std::ops::Deref for Word<T> {
    type Target = WordLimbs<T, 2>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<F: FieldExt> Word<F> {
    pub fn from_u256(u256: eth_types::Word) -> Word<F> {
        let bytes = u256.to_le_bytes();
        Word::new([
            from_bytes::value(&bytes[..16]),
            from_bytes::value(&bytes[16..]),
        ])
    }
}

impl<F: FieldExt> Word<Expression<F>> {
    pub fn from_lo(lo: Expression<F>) -> Self {
        Self(WordLimbs::<Expression<F>, 2>::new([lo, 0.expr()]))
    }

    pub fn zero() -> Self {
        Self(WordLimbs::<Expression<F>, 2>::new([0.expr(), 0.expr()]))
    }

    // select based on selector. Here assume selector is 1/0 therefore no overflow check
    pub fn select(
        selector: Expression<F>,
        when_true: Word<Expression<F>>,
        when_false: Word<Expression<F>>,
    ) -> Self {
        let (true_lo, true_hi) = when_true.mul_selector(selector.clone()).to_lo_hi();
        let (false_lo, false_hi) = when_false.mul_selector(1.expr() - selector).to_lo_hi();
        Word::new([
            true_lo.clone() + false_lo.clone(),
            true_hi.clone() + false_hi.clone(),
        ])
    }

    // Assume selector is 1/0 therefore no overflow check
    fn mul_selector(&self, selector: Expression<F>) -> Self {
        Word::new([self.lo().clone() * selector, self.hi().clone() * selector])
    }
}

impl<F: FieldExt, const N1: usize> WordLimbs<Expression<F>, N1> {
    // TODO static assertion. wordaround https://github.com/nvzqz/static-assertions-rs/issues/40
    pub fn to_wordlimbs<const N2: usize>(&self) -> WordLimbs<Expression<F>, N2> {
        assert_eq!(N1 % N2, 0);
        let limbs = self
            .limbs
            .chunks(N1 / N2)
            .map(|chunk| from_bytes::expr(chunk))
            .collect_vec()
            .try_into()
            .unwrap();
        WordLimbs::<Expression<F>, N2>::new(limbs)
    }

    pub fn to_word(&self) -> Word<Expression<F>> {
        Word(self.to_wordlimbs())
    }

    // TODO static assertion. wordaround https://github.com/nvzqz/static-assertions-rs/issues/40
    pub fn is_eq<const N2: usize>(&self, others: &WordLimbs<Expression<F>, N2>) -> Expression<F> {
        assert_eq!(N1 % N2, 0);
        not::expr(or::expr(
            self.limbs
                .chunks(N1 / N2)
                .map(|chunk| from_bytes::expr(chunk))
                .zip(others.limbs)
                .map(|(expr1, expr2)| expr1 - expr2)
                .collect_vec(),
        ))
    }
}
