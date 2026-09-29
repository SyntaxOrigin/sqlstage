//! Sorgu yürütücüsü: tek tablo tam taraması, filtre, sıralama ve dilimleme.
//!
//! Yürütücü satırları akış hâlinde değil, vektör olarak bellekte tutar; bu, öğrenme aracı
//! ölçeğindeki veritabanları (on binlerce satır) için yeterlidir ve ızgarayı basit tutar.
//! Büyük sonuçlarda bellek bütçesi aşımı `## Bilinen Sınırlamalar` bölümünde belgelidir.

use std::collections::HashMap;
use std::time::Instant;

use crate::deger::Deger;
use crate::hata::SahneHata;
use crate::sema::{sema_oku, Sema, Tablo};
use crate::sql::ast::Sorgu;
use crate::sql::filtre::degerlendir;
use crate::veritabani::Veritabani;

/// Sonuç ızgarasının bir kolon başlığı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SutunBasligi {
    /// Görünen başlık metni (`AS` takma adı olabilir).
    pub ad: String,
    /// Şemadan gelen tip adı (`INTEGER`, `TEXT` ...).
    pub tur: String,
}

/// Bir `SELECT` sorgusunun tamamlanmış çıktısı.
#[derive(Debug, Clone, PartialEq)]
pub struct SorguSonucu {
    /// Sonuç kolonlarının başlıkları.
    pub sutunlar: Vec<SutunBasligi>,
    /// Sonuç satırları.
    pub satirlar: Vec<Vec<Deger>>,
    /// Tablodan okunan toplam satır sayısı.
    pub taranan_satir: u64,
    /// Filtreyi geçen satır sayısı (dilimleme öncesi).
    pub eslesen_satir: u64,
    /// Yürütme süresi (nanosecond, `std::time::Instant` ile ölçülür).
    pub sure_ns: u128,
}

impl SorguSonucu {
    /// Sonuç satırı sayısını döndürür.
    pub fn satir_sayisi(&self) -> usize {
        self.satirlar.len()
    }
}

/// Sorguyu verilen veritabanı üzerinde çalıştırır.
///
/// # Hatalar
///
/// Tablo veya kolon bulunamazsa, b-tree bozuksa, sıralama kolonu geçersizse veya sorgu
/// metni desteklenen alt kümenin dışındaysa hata döner.
pub fn calistir(db: &Veritabani, sorgu: &Sorgu) -> Result<SorguSonucu, SahneHata> {
    let baslangic = Instant::now();
    let sema = sema_oku(db)?;
    let tablo = tablo_bul(&sema, &sorgu.tablo)?;
    let kolonlar = tablo.kolon_haritasi();

    // Sıralama ve filtre kolonları önceden doğrulanır: hatayı veri okumadan önce veriyoruz.
    for madde in &sorgu.siralama {
        if !kolonlar.contains_key(&madde.sutun.to_ascii_lowercase()) {
            return Err(SahneHata::KolonYok {
                tablo: tablo.ad.clone(),
                kolon: madde.sutun.clone(),
            });
        }
    }
    if let Some(filtre) = &sorgu.filtre {
        filtre_kolonlarini_dogrula(filtre, &kolonlar, &tablo.ad)?;
    }
    for projeksiyon in projeksiyonlar(&sorgu.secim) {
        if !kolonlar.contains_key(&projeksiyon.sutun.to_ascii_lowercase()) {
            return Err(SahneHata::KolonYok {
                tablo: tablo.ad.clone(),
                kolon: projeksiyon.sutun.clone(),
            });
        }
    }

    let ham = db.tablo_satirlari(tablo.kok_sayfa)?;
    let taranan = ham.len() as u64;

    let mut secilen: Vec<&crate::veritabani::Satir> = Vec::new();
    match &sorgu.filtre {
        None => secilen.extend(ham.iter()),
        Some(filtre) => {
            for satir in &ham {
                if degerlendir(filtre, &satir.degerler, &kolonlar)?.dogru_mu() {
                    secilen.push(satir);
                }
            }
        }
    }
    let eslesen = secilen.len() as u64;

    // ORDER BY: kaynak satırlar üzerinde, SQLite sırasına uyarak kararlı sıralama.
    for madde in sorgu.siralama.iter().rev() {
        let indeks = kolonlar[&madde.sutun.to_ascii_lowercase()];
        let azalan = madde.azalan;
        secilen.sort_by(|a, b| {
            let sol = a.degerler.get(indeks).unwrap_or(&Deger::Null);
            let sag = b.degerler.get(indeks).unwrap_or(&Deger::Null);
            let sira = sol.karsilastir(sag);
            if azalan {
                sira.reverse()
            } else {
                sira
            }
        });
    }

    // OFFSET / LIMIT
    let ofset = sorgu.ofset.unwrap_or(0);
    let dilim: Vec<&crate::veritabani::Satir> = secilen
        .into_iter()
        .skip(ofset)
        .take(sorgu.limit.unwrap_or(usize::MAX))
        .collect();

    let (sutunlar, kaynak_indeksleri) = basliklar_ve_indeksler(&sorgu.secim, tablo)?;
    let satirlar: Vec<Vec<Deger>> = dilim
        .iter()
        .map(|satir| {
            kaynak_indeksleri
                .iter()
                .map(|i| {
                    // `INTEGER PRIMARY KEY` kolonu bir rowid alias'tır: kayıtta NULL
                    // durur, okunacak değer satır kimliğidir (SQLite ile aynı davranış).
                    if tablo.rowid_alan == Some(*i) {
                        Deger::Tam(satir.satir_kimligi)
                    } else {
                        satir.deger(*i).cloned().unwrap_or(Deger::Null)
                    }
                })
                .collect()
        })
        .collect();

    Ok(SorguSonucu {
        sutunlar,
        satirlar,
        taranan_satir: taranan,
        eslesen_satir: eslesen,
        sure_ns: baslangic.elapsed().as_nanos(),
    })
}

/// Sorguda geçen tabloyu şemadan bulur.
pub fn tablo_bul<'a>(sema: &'a Sema, ad: &str) -> Result<&'a Tablo, SahneHata> {
    sema.tablo(ad).ok_or_else(|| SahneHata::TabloYok {
        ad: ad.to_string(),
        mevcut: sema.tablolar.iter().map(|t| t.ad.clone()).collect(),
    })
}

/// Sorgunun `WHERE` ifadesinde geçen kolon adlarını doğrular.
fn filtre_kolonlarini_dogrula(
    ifade: &crate::sql::ast::Ifade,
    kolonlar: &HashMap<String, usize>,
    tablo_adi: &str,
) -> Result<(), SahneHata> {
    use crate::sql::ast::Ifade;
    match ifade {
        Ifade::Karsilastirma { sutun, .. }
        | Ifade::Benzer { sutun, .. }
        | Ifade::NullMu { sutun, .. } => {
            if !kolonlar.contains_key(&sutun.to_ascii_lowercase()) {
                return Err(SahneHata::KolonYok {
                    tablo: tablo_adi.to_string(),
                    kolon: sutun.clone(),
                });
            }
            Ok(())
        }
        Ifade::Ve(hepler) | Ifade::Ya(hepler) => {
            for hepler in hepler {
                filtre_kolonlarini_dogrula(hepler, kolonlar, tablo_adi)?;
            }
            Ok(())
        }
        Ifade::Tumu { ic, .. } => filtre_kolonlarini_dogrula(ic, kolonlar, tablo_adi),
    }
}

/// Sorgunun projeksiyon listesini döndürür (`*` durumunda tablonun tüm kolonları).
fn projeksiyonlar(secim: &crate::sql::ast::Secim) -> Vec<crate::sql::ast::Projeksiyon> {
    match secim {
        crate::sql::ast::Secim::Yildiz => Vec::new(),
        crate::sql::ast::Secim::Sutunlar(l) => l.clone(),
    }
}

/// Sonuç başlıklarını ve her başlığın kaynak kolon dizinini üretir.
fn basliklar_ve_indeksler(
    secim: &crate::sql::ast::Secim,
    tablo: &Tablo,
) -> Result<(Vec<SutunBasligi>, Vec<usize>), SahneHata> {
    let indeks_bul = |ad: &str| -> Result<usize, SahneHata> {
        tablo
            .kolon(ad)
            .and_then(|k| {
                tablo
                    .kolon_haritasi()
                    .get(&k.ad.to_ascii_lowercase())
                    .copied()
            })
            .ok_or_else(|| SahneHata::KolonYok {
                tablo: tablo.ad.clone(),
                kolon: ad.to_string(),
            })
    };
    match secim {
        crate::sql::ast::Secim::Yildiz => {
            let basliklar = tablo
                .kolonlar
                .iter()
                .map(|k| SutunBasligi {
                    ad: k.ad.clone(),
                    tur: k.tur.ad().to_string(),
                })
                .collect();
            let indeksler = (0..tablo.kolonlar.len()).collect();
            Ok((basliklar, indeksler))
        }
        crate::sql::ast::Secim::Sutunlar(l) => {
            let mut basliklar = Vec::with_capacity(l.len());
            let mut indeksler = Vec::with_capacity(l.len());
            for projeksiyon in l {
                let indeks = indeks_bul(&projeksiyon.sutun)?;
                let tur = tablo
                    .kolon(&projeksiyon.sutun)
                    .map_or_else(|| "?".to_string(), |k| k.tur.ad().to_string());
                basliklar.push(SutunBasligi {
                    ad: projeksiyon.baslik().to_string(),
                    tur,
                });
                indeksler.push(indeks);
            }
            Ok((basliklar, indeksler))
        }
    }
}

/// Bir `SELECT` sorgusunu metinden ayrıştırıp çalıştırır.
pub fn metin_ile_calistir(db: &Veritabani, metin: &str) -> Result<SorguSonucu, SahneHata> {
    let sorgu = crate::sql::parser::ayikla(metin)?;
    calistir(db, &sorgu)
}
