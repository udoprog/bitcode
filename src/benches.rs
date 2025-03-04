use crate::Buffer;
use crate::{Decode, Encode};
use bincode::Options;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use paste::paste;
use rand::distributions::Alphanumeric;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use test::{black_box, Bencher};

// type StringImpl = arrayvec::ArrayString<16>;
type StringImpl = String;

#[derive(Debug, Default, PartialEq, Encode, Decode, Serialize, Deserialize)]
struct Data {
    #[bitcode_hint(expected_range = "0.0..1.0")]
    x: Option<f32>,
    y: Option<i8>,
    z: u16,
    s: StringImpl,
    e: DataEnum,
}

fn gen_len(r: &mut (impl Rng + ?Sized)) -> usize {
    (r.gen::<f32>().powi(4) * 16.0) as usize
}

impl Distribution<Data> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Data {
        let n = gen_len(rng);
        Data {
            x: rng.gen_bool(0.15).then(|| rng.gen()),
            y: rng.gen_bool(0.3).then(|| rng.gen()),
            z: rng.gen(),
            s: StringImpl::try_from(
                rng.sample_iter(Alphanumeric)
                    .take(n)
                    .map(char::from)
                    .collect::<String>()
                    .as_str(),
            )
            .unwrap(),
            e: rng.gen(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Encode, Decode, Serialize, Deserialize)]
enum DataEnum {
    #[default]
    #[bitcode_hint(frequency = 10)]
    Bar,
    Baz(StringImpl),
    Foo(Option<u8>),
}

impl Distribution<DataEnum> for rand::distributions::Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> DataEnum {
        if rng.gen_bool(0.9) {
            DataEnum::Bar
        } else if rng.gen_bool(0.5) {
            let n = gen_len(rng);
            DataEnum::Baz(
                StringImpl::try_from(
                    rng.sample_iter(Alphanumeric)
                        .take(n)
                        .map(char::from)
                        .collect::<String>()
                        .as_str(),
                )
                .unwrap(),
            )
        } else {
            DataEnum::Foo(rng.gen_bool(0.5).then(|| rng.gen()))
        }
    }
}

fn random_data(n: usize) -> Vec<Data> {
    let mut rng = ChaCha20Rng::from_seed(Default::default());
    (0..n).map(|_| rng.gen()).collect()
}

fn bitcode_encode(v: &(impl Encode + ?Sized)) -> Vec<u8> {
    crate::encode(v).unwrap()
}

fn bitcode_decode<T: Decode>(v: &[u8]) -> T {
    crate::decode(v).unwrap()
}

fn bitcode_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    crate::serde::serialize(v).unwrap()
}

fn bitcode_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    crate::serde::deserialize(v).unwrap()
}

fn bincode_fixint_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    bincode::serialize(v).unwrap()
}

fn bincode_fixint_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::deserialize(v).unwrap()
}

fn bincode_varint_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    bincode::DefaultOptions::new().serialize(v).unwrap()
}

fn bincode_varint_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new().deserialize(v).unwrap()
}

fn bincode_lz4_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    compress_prepend_size(&bincode::DefaultOptions::new().serialize(v).unwrap())
}

fn bincode_lz4_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new()
        .deserialize(&decompress_size_prepended(v).unwrap())
        .unwrap()
}

fn bincode_flate2_fast_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::fast());
    bincode::DefaultOptions::new()
        .serialize_into(&mut e, v)
        .unwrap();
    e.finish().unwrap()
}

fn bincode_flate2_fast_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode::DefaultOptions::new()
        .deserialize_from(DeflateDecoder::new(v))
        .unwrap()
}

fn bincode_flate2_best_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
    bincode::DefaultOptions::new()
        .serialize_into(&mut e, v)
        .unwrap();
    e.finish().unwrap()
}

fn bincode_flate2_best_deserialize<T: DeserializeOwned>(v: &[u8]) -> T {
    bincode_flate2_fast_deserialize(v)
}

fn postcard_serialize(v: &(impl Serialize + ?Sized)) -> Vec<u8> {
    postcard::to_allocvec(v).unwrap()
}

fn postcard_deserialize<T: DeserializeOwned>(buf: &[u8]) -> T {
    postcard::from_bytes(buf).unwrap()
}

fn bench_data() -> Vec<Data> {
    random_data(1000)
}

fn bench_serialize(b: &mut Bencher, ser: fn(&[Data]) -> Vec<u8>) {
    let data = bench_data();
    b.iter(|| {
        black_box(ser(black_box(&data)));
    })
}

fn bench_deserialize(b: &mut Bencher, ser: fn(&[Data]) -> Vec<u8>, de: fn(&[u8]) -> Vec<Data>) {
    let data = bench_data();
    let ref serialized_data = ser(&data);
    assert_eq!(de(serialized_data), data);
    b.iter(|| {
        black_box(de(black_box(serialized_data)));
    })
}

#[bench]
fn bench_bitcode_buffer_serialize(b: &mut Bencher) {
    let data = bench_data();
    let mut buf = Buffer::new();
    buf.serialize(&data).unwrap();
    let initial_cap = buf.capacity();
    b.iter(|| {
        black_box(buf.serialize(black_box(&data)).unwrap());
    });
    assert_eq!(buf.capacity(), initial_cap);
}

#[bench]
fn bench_bitcode_buffer_deserialize(b: &mut Bencher) {
    let data = bench_data();
    let ref bytes = crate::serde::serialize(&data).unwrap();
    let mut buf = Buffer::new();
    assert_eq!(buf.deserialize::<Vec<Data>>(bytes).unwrap(), data);
    let initial_cap = buf.capacity();
    b.iter(|| {
        black_box(buf.deserialize::<Vec<Data>>(black_box(bytes)).unwrap());
    });
    assert_eq!(buf.capacity(), initial_cap);
}

#[bench]
fn bench_bitcode_long_string_serialize(b: &mut Bencher) {
    let data = "abcde12345".repeat(1000);
    let mut buf = Buffer::new();
    buf.serialize(&data).unwrap();
    b.iter(|| {
        black_box(buf.serialize(black_box(&data)).unwrap());
    });
}

#[bench]
fn bench_bitcode_long_string_deserialize(b: &mut Bencher) {
    let data = "abcde12345".repeat(1000);
    let mut buf = Buffer::new();
    let bytes = buf.serialize(&data).unwrap().to_vec();
    assert_eq!(buf.deserialize::<String>(&bytes).unwrap(), data);
    b.iter(|| {
        black_box(buf.deserialize::<String>(black_box(&bytes)).unwrap());
    });
}

macro_rules! bench {
    ($serialize:ident, $deserialize:ident, $($name:ident),*) => {
        paste! {
            $(
                #[bench]
                fn [<bench_ $name _$serialize>] (b: &mut Bencher) {
                    bench_serialize(b, [<$name _ $serialize>])
                }

                #[bench]
                fn [<bench_ $name _$deserialize>] (b: &mut Bencher) {
                    bench_deserialize(b, [<$name _ $serialize>], [<$name _ $deserialize>])
                }
            )*
        }
    }
}

mod derive {
    use super::*;
    bench!(encode, decode, bitcode);
}

bench!(
    serialize,
    deserialize,
    bitcode,
    bincode_fixint,
    bincode_varint,
    bincode_lz4,
    bincode_flate2_fast,
    bincode_flate2_best,
    postcard
);

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[test]
    fn comparison1() {
        let ref data = random_data(10000);
        let print_results = |name: &'static str, b: Vec<u8>| {
            let zeros = 100.0 * b.iter().filter(|&&b| b == 0).count() as f32 / b.len() as f32;
            let precision = 2 - (zeros.log10().ceil() as usize).min(1);

            println!(
                "| {name:<22} | {:<12.1} | {zeros:>4.1$}%      |",
                b.len() as f32 / data.len() as f32,
                precision,
            );
        };

        println!("| Format                 | Size (bytes) | Zero Bytes |");
        println!("|------------------------|--------------|------------|");
        print_results("Bitcode (derive)", bitcode_encode(data));
        print_results("Bitcode (serde)", bitcode_serialize(data));
        print_results("Bincode", bincode_fixint_serialize(data));
        print_results("Bincode (varint)", bincode_varint_serialize(data));

        // These use varint since it makes the result smaller and actually speeds up compression.
        print_results("Bincode (LZ4)", bincode_lz4_serialize(data));
        print_results(
            "Bincode (Deflate Fast)",
            bincode_flate2_fast_serialize(data),
        );
        print_results(
            "Bincode (Deflate Best)",
            bincode_flate2_best_serialize(data),
        );

        // TODO compressed postcard.
        print_results("Postcard", postcard_serialize(data));

        println!(
            "| ideal (max entropy)    |              | {:.2}%      |",
            100.0 / 255.0
        );
    }

    #[test]
    fn comparison2() {
        use std::ops::RangeInclusive;

        fn compare<T: Encode + Serialize + Clone>(name: &str, r: RangeInclusive<T>) {
            fn measure<T: Encode + Serialize + Clone>(t: T) -> [usize; 5] {
                const COUNT: usize = 8;
                let many: [T; COUNT] = std::array::from_fn(|_| t.clone());
                [
                    bitcode_encode(&many).len(),
                    bitcode_serialize(&many).len(),
                    bincode_fixint_serialize(&many).len(),
                    bincode_varint_serialize(&many).len(),
                    postcard_serialize(&many).len(),
                ]
                .map(|b| 8 * b / COUNT)
            }

            let lo = measure(r.start().clone());
            let hi = measure(r.end().clone());

            let v: Vec<_> = lo
                .into_iter()
                .zip(hi)
                .map(|(lo, hi)| {
                    if lo == hi {
                        format!("{lo}")
                    } else {
                        format!("{lo}-{hi}")
                    }
                })
                .collect();
            println!(
                "| {name:<19} | {:<16} | {:<15} | {:<7} | {:<16} | {:<8} |",
                v[0], v[1], v[2], v[3], v[4],
            );
        }

        fn compare_one<T: Encode + Serialize + Clone>(name: &str, t: T) {
            compare(name, t.clone()..=t);
        }

        #[derive(Clone, Encode, Decode, Serialize, Deserialize)]
        enum Enum {
            A,
            B,
            C,
            D,
        }

        println!("| Type                | Bitcode (derive) | Bitcode (serde) | Bincode | Bincode (varint) | Postcard |");
        println!("|---------------------|------------------|-----------------|---------|------------------|----------|");
        compare("bool", false..=true);
        compare("u8", 0u8..=u8::MAX);
        compare("i8", 0i8..=i8::MAX);
        compare("u16", 0u16..=u16::MAX);
        compare("i16", 0i16..=i16::MAX);
        compare("u32", 0u32..=u32::MAX);
        compare("i32", 0i32..=i32::MAX);
        compare("u64", 0u64..=u64::MAX);
        compare("i64", 0i64..=i64::MAX);
        compare_one("f32", 0f32);
        compare_one("f64", 0f64);
        compare("char", (0 as char)..=char::MAX);
        compare("Option<()>", None..=Some(()));
        compare("Result<(), ()>", Ok(())..=Err(()));
        compare("enum { A, B, C, D }", Enum::A..=Enum::D);

        println!();
        println!("| Value               | Bitcode (derive) | Bitcode (serde) | Bincode | Bincode (varint) | Postcard |");
        println!("|---------------------|------------------|-----------------|---------|------------------|----------|");
        compare_one("[true; 4]", [true; 4]);
        compare_one("vec![(); 0]", vec![(); 0]);
        compare_one("vec![(); 1]", vec![(); 1]);
        compare_one("vec![(); 256]", vec![(); 256]);
        compare_one("vec![(); 65536]", vec![(); 65536]);
        compare_one(r#""""#, "");
        compare_one(r#""abcd""#, "abcd");
        compare_one(r#""abcd1234""#, "abcd1234");
    }
}
