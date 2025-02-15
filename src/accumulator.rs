use num_bigint::traits::{ExtendedGcd, ModInverse};
use num_bigint::{BigInt, BigUint, IntoBigUint};
use num_integer::Integer;
use num_traits::{One, Zero};
use rand::CryptoRng;
use rand::Rng;

use crate::math::{modpow_uint_int, root_factor, shamir_trick};
use crate::proofs;
use crate::traits::*;

// All accumulated values are small odd primes.
// Arbitrary data values can be hashed to small primes,
// It is also assumed that no item is added twice to the accumulator !!!
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct Accumulator {
    /// Length of the Integer we use in bits, This is Lambda and our security parameter
    int_size_bits: usize, //change this to u16

    /// An accumulator must have a public key divided into two parts.
    /// A common reference string pp generated by the Setup algorithm in place of private/public keys.
    /// Generator
    g: BigUint,

    /// Our Modulus, generated by using a public randomness known by the adversary
    n: BigUint,

    /// Current accumulator state
    root: BigUint,

    /// The set of elements currently accumulated (product of the current set)
    set: BigUint,
}

impl Accumulator {}

impl StaticAccumulator for Accumulator {
    /// Returns the current public state.
    fn state(&self) -> &BigUint {
        &self.root
    }

    /// Generates a group of unknown order and initializes the group with a generator of that group.
    /// Setup(λ, z) → pp, A0 Generate the public parameters
    fn setup<T, R>(rng: &mut R, int_size_bits: usize) -> Self
    where
        T: PrimeGroup,
        R: CryptoRng + Rng,
    {
        // Generate n = p q, |n| = int_size_bits
        // This is a trusted setup, as we do know `p` and `q`, even though
        // we choose not to store them.

        let (n, g) = T::generate_primes(rng, int_size_bits).unwrap();

        Accumulator {
            int_size_bits,
            root: g.clone(),
            g,
            n,
            set: BigUint::one(),
        }
    }

    ///Takes the current accumulator At, an element from the odd primes domain, and computes At+1 = At.
    #[inline]
    fn add(&mut self, x: &BigUint) {
        debug_assert!(
            self.g.clone().modpow(&self.set, &self.n) == self.root,
            "invalid state - pre add"
        );

        // assumes x is already a prime
        self.set *= x;
        self.root = self.root.modpow(x, &self.n);
    }

    //A membership witness is simply the accumulator without the aggregated item.
    #[inline]
    fn mem_wit_create(&self, x: &BigUint) -> BigUint {
        debug_assert!(
            self.g.clone().modpow(&self.set, &self.n) == self.root,
            "invalid state"
        );

        let (set, r) = self.set.clone().div_rem(x);
        debug_assert!(r.is_zero(), "x was not a valid member of set");

        self.g.clone().modpow(&set, &self.n)
    }

    #[inline]
    fn ver_mem(&self, w: &BigUint, x: &BigUint) -> bool {
        w.modpow(x, &self.n) == self.root
    }
}

impl DynamicAccumulator for Accumulator {
    #[inline]
    fn del(&mut self, x: &BigUint) -> Option<()> {
        let old_s = self.set.clone();
        self.set /= x;

        if self.set == old_s {
            return None;
        }

        self.root = self.g.clone().modpow(&self.set, &self.n); //Returns (self ^ exponent) % modulus.
        Some(())
    }
}

impl UniversalAccumulator for Accumulator {
    fn non_mem_wit_create(&self, x: &BigUint) -> (BigUint, BigInt) {
        // set* <- \prod_{set\in S} set
        let s_star = &self.set;

        // a, b <- Bezout(x, set*)
        let (_, a, b) = ExtendedGcd::extended_gcd(x, s_star);
        let d = modpow_uint_int(&self.g, &a, &self.n).expect("prime");

        (d, b)
    }

    fn ver_non_mem(&self, w: &(BigUint, BigInt), x: &BigUint) -> bool {
        let (d, b) = w;

        // A^b
        let a_b = modpow_uint_int(&self.root, b, &self.n).expect("prime");
        // d^x
        let d_x = d.modpow(x, &self.n);

        // d^x A^b == g
        (d_x * &a_b) % &self.n == self.g
    }
}

impl BatchedAccumulator for Accumulator {
    fn batch_add(&mut self, xs: &[BigUint]) -> BigUint {
        //begin our summation of the added elements
        let mut x_star = BigUint::one();
        for x in xs {
            x_star *= x;
            //add into element
            self.set *= x;
        }

        //temp clone our old root
        let root_t = self.root.clone();
        //calculate our new root after all the added elements
        self.root = self.root.modpow(&x_star, &self.n); //Returns (self ^ exponent) % modulus.
                                                        //create our proof for the procedure
        proofs::ni_poe_prove(&x_star, &root_t, &self.root, &self.n)
    }

    fn ver_batch_add(&self, w: &BigUint, root: &BigUint, xs: &[BigUint]) -> bool {
        let mut x_star = BigUint::one();
        for x in xs {
            x_star *= x
        }

        proofs::ni_poe_verify(&x_star, root, &self.root, &w, &self.n)
    }

    fn batch_del(&mut self, pairs: &[(BigUint, BigUint)]) -> Option<BigUint> {
        if pairs.is_empty() {
            return None;
        }
        let mut pairs = pairs.iter();
        let root_t = self.root.clone();

        let (x0, w0) = pairs.next().unwrap();
        let mut x_star = x0.clone();
        let mut new_root = w0.clone();

        for (xi, wi) in pairs {
            new_root = shamir_trick(&new_root, wi, &x_star, xi, &self.n).unwrap();
            x_star *= xi;
            // for now this is not great, depends on this impl, not on the general design
            self.set /= xi;
        }

        self.root = new_root;

        Some(proofs::ni_poe_prove(&x_star, &self.root, &root_t, &self.n))
    }

    fn ver_batch_del(&self, w: &BigUint, root: &BigUint, xs: &[BigUint]) -> bool {
        let mut x_star = BigUint::one();
        for x in xs {
            x_star *= x
        }

        proofs::ni_poe_verify(&x_star, &self.root, root, &w, &self.n)
    }

    fn del_w_mem(&mut self, w: &BigUint, x: &BigUint) -> Option<()> {
        if !self.ver_mem(w, x) {
            return None;
        }

        self.set /= x;
        // w is root without x, so need to recompute
        self.root = w.clone();

        Some(())
    }

    #[inline]
    fn create_all_mem_wit(&self, set: &[BigUint]) -> Vec<BigUint> {
        root_factor(&self.g, &set, &self.n)
    }

    fn agg_mem_wit(
        &self,
        w_x: &BigUint,
        w_y: &BigUint,
        x: &BigUint,
        y: &BigUint,
    ) -> (BigUint, BigUint) {
        // TODO: check this matches, sth is not quite right in the paper here
        let w_xy = shamir_trick(w_x, w_y, x, y, &self.n).unwrap();
        let xy = x.clone() * y;

        debug_assert!(
            w_xy.modpow(&xy, &self.n) == self.root,
            "invalid shamir trick"
        );

        let pi = proofs::ni_poe_prove(&xy, &w_xy, &self.root, &self.n);

        (w_xy, pi)
    }

    fn ver_agg_mem_wit(&self, w_xy: &BigUint, pi: &BigUint, x: &BigUint, y: &BigUint) -> bool {
        let xy = x.clone() * y;
        proofs::ni_poe_verify(&xy, w_xy, &self.root, pi, &self.n)
    }

    fn mem_wit_create_star(&self, x: &BigUint) -> (BigUint, BigUint) {
        let w_x = self.mem_wit_create(x);
        debug_assert!(self.root != w_x, "{} was not a member", x);
        let p = proofs::ni_poe_prove(x, &w_x, &self.root, &self.n);

        (w_x, p)
    }

    fn ver_mem_star(&self, x: &BigUint, pi: &(BigUint, BigUint)) -> bool {
        proofs::ni_poe_verify(x, &pi.0, &self.root, &pi.1, &self.n)
    }

    fn mem_wit_x(
        &self,
        _other: &BigUint,
        w_x: &BigUint,
        w_y: &BigUint,
        _x: &BigUint,
        _y: &BigUint,
    ) -> BigUint {
        (w_x * w_y) % &self.n
    }

    fn ver_mem_x(&self, other: &BigUint, pi: &BigUint, x: &BigUint, y: &BigUint) -> bool {
        // assert x and y are coprime
        let q = x.gcd(y);
        if !q.is_one() {
            return false;
        }

        // A_1^y
        let rhs_a = self.root.modpow(y, &self.n);
        // A_2^x
        let rhs_b = other.modpow(x, &self.n);

        // A_1^y * A_2^x
        let rhs = (rhs_a * rhs_b) % &self.n;
        // pi^{x * y}
        let lhs = pi.modpow(&(x.clone() * y), &self.n);

        lhs == rhs
    }

    fn non_mem_wit_create_star(
        &self,
        x: &BigUint,
    ) -> (BigUint, BigUint, (BigUint, BigUint, BigInt), BigUint) {
        let g = &self.g;
        let n = &self.n;

        // a, b <- Bezout(x, s_star)
        let (_, a, b) = ExtendedGcd::extended_gcd(x, &self.set);

        // d <- g^a
        let d = modpow_uint_int(g, &a, n).expect("invalid state");
        // v <- A^b
        let v = modpow_uint_int(&self.root, &b, n).expect("invalid state");

        // pi_d <- NI-PoKE2(b, A, v)
        let pi_d = proofs::ni_poke2_prove(b, &self.root, &v, n);

        // k <- g * v^-1
        let k = (g * v
            .clone()
            .mod_inverse(n)
            .expect("invalid state")
            .into_biguint()
            .unwrap())
            % n;

        // pi_g <- NI-PoE(x, d, g * v^-1)
        let pi_g = proofs::ni_poe_prove(x, &d, &k, n);

        // return {d, v, pi_d, pi_g}
        (d, v, pi_d, pi_g)
    }

    fn ver_non_mem_star(
        &self,
        x: &BigUint,
        pi: &(BigUint, BigUint, (BigUint, BigUint, BigInt), BigUint),
    ) -> bool {
        let g = &self.g;
        let n = &self.n;

        let (d, v, pi_d, pi_g) = pi;

        // verify NI-PoKE2
        if !proofs::ni_poke2_verify(&self.root, &v, pi_d, n) {
            return false;
        }

        // verify NI-PoE
        let k = (g * v
            .clone()
            .mod_inverse(n)
            .expect("invalid state")
            .into_biguint()
            .unwrap())
            % n;

        if !proofs::ni_poe_verify(x, d, &k, pi_g, n) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::group::RSAGroup;
    use num_bigint::RandPrime;
    use num_bigint::Sign;
    use num_traits::FromPrimitive;
    use rand::SeedableRng;
    use rand_chacha::ChaChaRng;

    #[test]
    fn test_static() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        for _ in 0..100 {
            let int_size_bits = 256; // insecure, but faster tests
            let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

            let xs = (0..5)
                .map(|_| rng.gen_prime(int_size_bits))
                .collect::<Vec<_>>();

            for x in &xs {
                acc.add(x);
            }

            for x in &xs {
                let w = acc.mem_wit_create(x);
                assert!(acc.ver_mem(&w, x));
            }
        }
    }

    #[test]
    fn test_dynamic() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        for _ in 0..20 {
            let int_size_bits = 256; // insecure, but faster tests
            let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

            let xs = (0..5)
                .map(|_| rng.gen_prime(int_size_bits))
                .collect::<Vec<_>>();

            for x in &xs {
                acc.add(x);
            }

            let ws = xs
                .iter()
                .map(|x| {
                    let w = acc.mem_wit_create(x);
                    assert!(acc.ver_mem(&w, x));
                    w
                })
                .collect::<Vec<_>>();

            for (x, w) in xs.iter().zip(ws.iter()) {
                // remove x
                acc.del(x).unwrap();
                // make sure test now fails
                assert!(!acc.ver_mem(w, x));
            }
        }
    }

    #[test]
    fn test_universal() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        for _ in 0..20 {
            let int_size_bits = 256; // insecure, but faster tests
            let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

            let xs = (0..5)
                .map(|_| rng.gen_prime(int_size_bits))
                .collect::<Vec<_>>();

            for x in &xs {
                acc.add(x);
            }

            for _ in 0..5 {
                let y = rng.gen_prime(int_size_bits);

                let w = acc.non_mem_wit_create(&y);
                assert!(acc.ver_non_mem(&w, &y));
            }
        }
    }

    #[test]
    fn test_math_non_mempership() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        let int_size_bits = 32;

        let x = rng.gen_prime(int_size_bits);
        let s1 = rng.gen_prime(int_size_bits);
        let s2 = rng.gen_prime(int_size_bits);

        let n = BigUint::from_u32(43 * 67).unwrap();
        let g = BigUint::from_u32(49).unwrap();

        // set* = \prod set
        let mut s_star = BigUint::one();
        s_star *= &s1;
        s_star *= &s2;

        // A = g ^ set*
        let root = g.modpow(&s_star, &n);

        let (_, a, b) = ExtendedGcd::extended_gcd(&x, &s_star);
        println!("{} {} {} {}", &g, &a, &b, &n);

        let u = BigInt::from_biguint(Sign::Plus, x.clone());
        let v = BigInt::from_biguint(Sign::Plus, s_star);
        let lhs = a.clone() * &u;
        let rhs = b.clone() * &v;
        println!("> {} * {} + {} * {} == 1", &a, &u, &b, &v);
        assert_eq!(lhs + &rhs, BigInt::one());

        // d = g^a mod n
        let d = modpow_uint_int(&g, &a, &n).unwrap();
        println!("> {} = {}^{} mod {}", &d, &g, &a, &n);

        // A^b
        let a_b = modpow_uint_int(&root, &b, &n).unwrap();
        println!("> {} = {}^{} mod {}", &a_b, &root, &b, &n);

        // A^b == g^{set* * b}
        let res = modpow_uint_int(&g, &(&v * &b), &n).unwrap();
        println!("> {} = {}^({} * {}) mod {}", &res, &g, &v, &b, &n);
        assert_eq!(a_b, res);

        // d^x
        let d_x = d.modpow(&x, &n);
        println!("> (d_x) {} = {}^{} mod {}", &d_x, &d, &x, &n);

        // d^x == g^{a * x}
        let res = modpow_uint_int(&g, &(&a * &u), &n).unwrap();
        println!("> (d_x) {} = {}^({} * {}) mod {}", &res, &g, &a, &u, &n);
        assert_eq!(d_x, res);

        // d^x A^b == g
        let lhs = (&d_x * &a_b) % &n;
        println!("> {} = {} * {} mod {}", &lhs, &d_x, &a_b, &n);
        assert_eq!(lhs, g);
    }

    fn test_batch_add_size(size: usize) {
        println!("batch_add_size {}", size);
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        let int_size_bits = 256; // insecure, but faster tests
        let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

        // regular add
        let x0 = rng.gen_prime(int_size_bits);
        acc.add(&x0);

        // batch add
        let root = acc.state().clone();
        let xs = (0..size)
            .map(|_| rng.gen_prime(int_size_bits))
            .collect::<Vec<_>>();
        let w = acc.batch_add(&xs);

        // verify batch add
        assert!(acc.ver_batch_add(&w, &root, &xs), "ver_batch_add failed");

        // delete with member
        let x = &xs[2];
        let w = acc.mem_wit_create(x);
        assert!(acc.ver_mem(&w, x), "failed to verify valid witness");

        acc.del_w_mem(&w, x).unwrap();
        assert!(
            !acc.ver_mem(&w, x),
            "witness verified, even though it was deleted"
        );

        // create all members witness
        // current state contains xs\x + x0
        let mut set = vec![x0.clone(), xs[0].clone(), xs[1].clone()];
        set.extend(xs.iter().skip(3).cloned());

        let ws = acc.create_all_mem_wit(&set);

        for (w, x) in ws.iter().zip(set.iter()) {
            assert!(acc.ver_mem(w, x));
        }

        // batch delete
        let root = acc.state().clone();
        let pairs = set
            .iter()
            .cloned()
            .zip(ws.iter().cloned())
            .take(3)
            .collect::<Vec<_>>();
        let w = acc.batch_del(&pairs[..]).unwrap();

        assert!(
            acc.ver_batch_del(&w, &root, &set[..3]),
            "ver_batch_del failed"
        );
    }

    #[test]
    fn test_batch_add_small() {
        for i in 4..14 {
            test_batch_add_size(i)
        }
    }

    #[test]
    fn test_batch_add_large() {
        let size = 128;
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);
        let int_size_bits = 256; // insecure, but faster tests
        let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

        // regular add
        let x0 = rng.gen_prime(int_size_bits);
        acc.add(&x0);

        // batch add
        let root = acc.state().clone();
        let xs = (0..size)
            .map(|_| rng.gen_prime(int_size_bits))
            .collect::<Vec<_>>();
        let w = acc.batch_add(&xs);

        // verify batch add
        assert!(acc.ver_batch_add(&w, &root, &xs), "ver_batch_add failed");

        // batch add
        let root = acc.state().clone();
        let xs = (0..size)
            .map(|_| rng.gen_prime(int_size_bits))
            .collect::<Vec<_>>();
        let w = acc.batch_add(&xs);

        // verify batch add
        assert!(acc.ver_batch_add(&w, &root, &xs), "ver_batch_add failed");
    }

    #[test]
    fn test_aggregation() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        for _ in 0..10 {
            let int_size_bits = 256; // insecure, but faster tests
            let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

            // regular add
            let xs = (0..5)
                .map(|_| rng.gen_prime(int_size_bits))
                .collect::<Vec<_>>();

            for x in &xs {
                acc.add(x);
            }

            // AggMemWit
            {
                let x = &xs[0];
                let y = &xs[1];
                let w_x = acc.mem_wit_create(x);
                let w_y = acc.mem_wit_create(y);

                let (w_xy, p_wxy) = acc.agg_mem_wit(&w_x, &w_y, x, y);

                assert!(
                    acc.ver_agg_mem_wit(&w_xy, &p_wxy, x, y),
                    "invalid agg_mem_wit proof"
                );
            }

            // MemWitCreate*
            {
                let pis = (0..5)
                    .map(|i| acc.mem_wit_create_star(&xs[i]))
                    .collect::<Vec<_>>();
                for (pi, x) in pis.iter().zip(&xs) {
                    assert!(acc.ver_mem_star(x, pi), "invalid mem_wit_create_star proof");
                }
            }

            // MemWitX
            {
                let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);
                let mut other = acc.clone();
                let x = rng.gen_prime(128);
                let y = rng.gen_prime(128);

                assert!(x.gcd(&y).is_one(), "x, y must be coprime");

                acc.add(&x);
                other.add(&y);

                let w_x = acc.mem_wit_create(&x);
                let w_y = other.mem_wit_create(&y);

                assert!(acc.ver_mem(&w_x, &x));
                assert!(other.ver_mem(&w_y, &y));

                let w_xy = acc.mem_wit_x(other.state(), &w_x, &w_y, &x, &y);
                assert!(
                    acc.ver_mem_x(other.state(), &w_xy, &x, &y),
                    "invalid ver_mem_x witness"
                );
            }
        }
    }

    #[test]
    fn test_aggregation_non_mem_star() {
        let rng = &mut ChaChaRng::from_seed([0u8; 32]);

        for _ in 0..10 {
            let int_size_bits = 256; // insecure, but faster tests
            let mut acc = Accumulator::setup::<RSAGroup, _>(rng, int_size_bits);

            // regular add
            let xs = (0..5)
                .map(|_| rng.gen_prime(int_size_bits))
                .collect::<Vec<_>>();

            for x in &xs {
                acc.add(x);
            }

            let x = rng.gen_prime(int_size_bits);
            let pi = acc.non_mem_wit_create_star(&x);

            assert!(acc.ver_non_mem_star(&x, &pi), "invalid ver_non_mem_star");
        }
    }
}
