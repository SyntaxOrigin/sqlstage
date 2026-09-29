//! SQLStage kütüphanesi: SQLite dosya biçiminin salt okunur okuyucusu ve daraltılmış
//! `SELECT` yorumlayıcısı.
//!
//! Bu crate hiçbir zaman veritabanı dosyasına yazmaz. Yazma yeteneği yalnızca `yazici`
//! modülünde, **kullanıcının açmadığı** yeni örnek veritabanı dosyaları üretmek için
//! vardır; okunan bir dosyayı değiştirmek için kullanılamaz.
//!
//! Sorumluluk dağılımı:
//!
//! - [`hata`]: tek hata tipi ve `Display` uygulaması.
//! - [`deger`]: SQLite dinamik tipleri (`NULL`, tamsayı, gerçek, metin, blob).
//! - [`varint`]: SQLite değişken uzunluklu tam sayı kodlaması (okuma + yazma).
//! - [`baslik`]: 100 baytlık veritabanı başlığının ayrıştırılması ve doğrulanması.
//! - [`sayfa`]: b-tree sayfa başlığı ve hücre dizisi ayrıştırması.
//! - [`kayit`]: kayıt (serial type) ayrıştırması ve varışma zinciri birleştirme.
//! - [`veritabani`]: salt okunur dosya erişimi, tablo taraması, bütünlük denetimi.
//! - [`sema`]: `sqlite_master` okuması, kolon modeli, `CREATE TABLE` alt kümesi ayrıştırıcısı.
//! - [`sql`]: SQL alt kümesi sözcükleri, AST, ayrıştırıcı, filtre ve yürütücü.
//! - [`plan`]: `EXPLAIN QUERY PLAN` yerine geçen maliyet tahmini metni.
//! - [`izgara`], [`json_cikti`], [`diyagram`]: terminal sonuç ızgarası, JSON dışa aktarım,
//!   ASCII şema diyagramı.
//! - [`yazici`]: yeni örnek veritabanı dosyası üreten minimal SQLite yazıcısı.
//! - [`ornek`], [`gecmis`]: örnek veri seti ve `sorgular.jsonl` sorgu geçmişi.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod baslik;
pub mod deger;
pub mod diyagram;
pub mod gecmis;
pub mod hata;
pub mod izgara;
pub mod json_cikti;
pub mod kayit;
pub mod ornek;
pub mod plan;
pub mod sayfa;
pub mod sema;
pub mod sql;
pub mod varint;
pub mod veritabani;
pub mod yazici;

pub use hata::SahneHata;
