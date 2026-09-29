//! SQL alt kümesi: sözcükler, soyut sözdizim ağacı, ayrıştırıcı, filtre değerlendirmesi ve
//! yürütücü.
//!
//! Desteklenen tek biçim şudur:
//!
//! ```text
//! SELECT <kolonlar> FROM <tek tablo> [WHERE <kosul>] [ORDER BY <kolon> [ASC|DESC]] [LIMIT n [OFFSET m]]
//! ```
//!
//! `JOIN`, alt sorgu, `UNION`, `GROUP BY`, toplam fonksiyonlar ve tüm yazma komutları
//! **açıkça reddedilir**; sessizce yorumlanmaz.

pub mod ast;
pub mod filtre;
pub mod lexer;
pub mod parser;
pub mod yurutucu;

pub use ast::{Ifade, KarsilastirmaIsleci, Projeksiyon, Sabit, Secim, Siralama, Sorgu};
pub use yurutucu::{SorguSonucu, SutunBasligi};
