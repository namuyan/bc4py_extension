#![feature(test)]
extern crate test;

extern crate bc4py_plotter;
extern crate bigint;
extern crate regex;

pub mod pymodules;
pub mod workhash;


#[cfg(test)]
mod tests {
    use super::bc4py_plotter::pochash::generator;
    use super::test::Bencher;
    use blake2b_simd::blake2bp::blake2bp;

    #[test]
    fn hash_function_check() {
        let address = "NDTTLPOUBQQLC5SZ4BPKK2GK6U3RP6TUKGBCCLDV";
        let nonce = 43521125;
        let hash = generator(address, nonce);
        let check = blake2bp(&hash[..]).to_hex();
        let expected = "6584ebca0cb8ce0eae31762a322f4ea762aa25948042f062376e8adc67c3efff\
                            08b84fe08254df63c1f3bfc1be8577271d80a24cadc275f4b4a4cadf80e170f3";
        assert_eq!(&check, expected);
    }

    #[bench]
    fn bench(b: &mut Bencher){
        // Sha256   174,143 ns/iter (+/- 30,521)
        // Sha512   102,946 ns/iter (+/- 29,696)
        // Sha3_256 131,604 ns/iter (+/- 53,561)
        // Sha3_512 258,106 ns/iter (+/- 199,488)
        // Blake2b   43,305 ns/iter (+/- 5,792)  64bytes
        // Blake2s   67,022 ns/iter (+/- 11,852) 32bytes
        // blake2b_simd blake2b   35,838 ns/iter (+/- 6,775) 64bytes
        // blake2b_simd blake2bp  18,853 ns/iter (+/- 6,379) 32bytes
        // caution! blake2b differ from blake2bp
        let x = [3u8;32768];
        b.iter(|| blake2bp(&x));
    }

    #[bench]
    fn box_ref(b: &mut Bencher){
        fn work() -> Box<[u8;32768*16]> {
            let mut data = Box::new([0u8;32768*16]);
            data.iter_mut().map(|x| *x += 1);
            data // 33,809 ns/iter (+/- 10,772)
        }
        b.iter(|| work());
    }

    #[bench]
    fn array_ref(b: &mut Bencher){
        fn work() -> [u8;32768*16] {
            let mut data = [0u8;32768*16];
            data.iter_mut().map(|x| *x += 1);
            data // 11,342 ns/iter (+/- 509)
        }
        b.iter(|| work());
    }
}
