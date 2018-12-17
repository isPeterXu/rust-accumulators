use blake2::Digest;
use generic_array::ArrayLength;
use num_bigint::BigUint;
use num_integer::Integer;
use rsa::prime::probably_prime;

/// Hash the given numbers to a prime number.
/// Currently uses only 128bits.
pub fn hash_prime<O: ArrayLength<u8>, D: Digest<OutputSize = O>>(input: &[u8]) -> BigUint {
    let mut y = BigUint::from_bytes_be(&D::digest(input)[..16]);

    while !probably_prime(&y, 20) {
        y = BigUint::from_bytes_be(&D::digest(&y.to_bytes_be())[..16]);
    }

    y
}

/// Hash the given numbers into the given group.
/// Only works for `OutputSize >= |n|`.
pub fn hash_group<O: ArrayLength<u8>, D: Digest<OutputSize = O>>(
    input: &[u8],
    n: &BigUint,
) -> BigUint {
    let y = BigUint::from_bytes_be(&D::digest(input)[..]);

    y.mod_floor(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    use blake2::Blake2b;
    use num_bigint::RandBigInt;
    use rand::{thread_rng, Rng};

    #[test]
    fn test_hash_prime() {
        let mut rng = thread_rng();

        for i in 1..10 {
            let mut val = vec![0u8; i * 32];
            rng.fill(&mut val[..]);

            let h = hash_prime::<_, Blake2b>(&val);
            assert!(probably_prime(&h, 20));
        }
    }

    #[test]
    fn test_hash_group() {
        let mut rng = thread_rng();

        for i in 1..10 {
            let mut val = vec![0u8; i * 32];
            rng.fill(&mut val[..]);
            let n = rng.gen_biguint(1024);

            let h = hash_group::<_, Blake2b>(&val, &n);
            assert!(h <= n);
        }
    }
}
