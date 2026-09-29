//! `WHERE` koşulunun satır başına değerlendirilmesi.
//!
//! SQL üç değerli mantık kullanır: bir karşılaştırmadan `NULL` çıkarsa sonuç `NULL`
//! (bilinmiyor) olur ve satır **seçilmez**. Bu, `WHERE a = NULL` ifadesinin hiçbir satırı
//! döndürmesinin nedenidir; doğru kullanım `WHERE a IS NULL` biçimidir.

use std::collections::HashMap;

use crate::deger::Deger;
use crate::sql::ast::{Ifade, KarsilastirmaIsleci};

/// Bir koşulun üç değerli sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sonuc {
    /// Koşul sağlandı.
    Dogru,
    /// Koşul sağlanmadı.
    Yanlis,
    /// Sonuç bilinmiyor (en az bir taraf `NULL`).
    Bilinmiyor,
}

impl Sonuc {
    /// Sonucu bool'a çevirir; `Bilinmiyor` değeri `false` olur.
    pub fn dogru_mu(&self) -> bool {
        matches!(self, Self::Dogru)
    }

    /// SQL üç değerli `AND` işlemi.
    pub fn ve(self, diger: Self) -> Self {
        match (self, diger) {
            (Self::Yanlis, _) | (_, Self::Yanlis) => Self::Yanlis,
            (Self::Bilinmiyor, _) | (_, Self::Bilinmiyor) => Self::Bilinmiyor,
            _ => Self::Dogru,
        }
    }

    /// SQL üç değerli `OR` işlemi.
    pub fn veya(self, diger: Self) -> Self {
        match (self, diger) {
            (Self::Dogru, _) | (_, Self::Dogru) => Self::Dogru,
            (Self::Bilinmiyor, _) | (_, Self::Bilinmiyor) => Self::Bilinmiyor,
            _ => Self::Yanlis,
        }
    }

    /// `NOT` işlemi (`NULL` sonuç `NULL` kalır).
    pub fn degil(self) -> Self {
        match self {
            Self::Dogru => Self::Yanlis,
            Self::Yanlis => Self::Dogru,
            Self::Bilinmiyor => Self::Bilinmiyor,
        }
    }
}

/// Kolon adlarını dizinlere eşleyen çözümleme tablosu.
pub type KolonHaritasi = HashMap<String, usize>;

/// `WHERE` ifadesini bir satır üzerinde değerlendirir.
///
/// Bilinmeyen kolon adı hata döndürür (sessizce `NULL` sayılmaz).
pub fn degerlendir(
    ifade: &Ifade,
    satir: &[Deger],
    kolonlar: &KolonHaritasi,
) -> Result<Sonuc, crate::hata::SahneHata> {
    match ifade {
        Ifade::Karsilastirma {
            sutun,
            islec,
            sabit,
        } => {
            let deger = kolon_degeri(sutun, satir, kolonlar)?;
            Ok(karsilastir(&deger, *islec, &sabit.deger()))
        }
        Ifade::Benzer {
            sutun,
            desen,
            degil,
        } => {
            let deger = kolon_degeri(sutun, satir, kolonlar)?;
            // SQLite'ta LIKE yalnızca metin üzerinde tanımlıdır; diğer tiplerde sonuç
            // NULL'dır ve satır seçilmez.
            let sonuc = match &deger {
                Deger::Metin(s) => {
                    if eslesir(s, desen) {
                        Sonuc::Dogru
                    } else {
                        Sonuc::Yanlis
                    }
                }
                _ => Sonuc::Bilinmiyor,
            };
            Ok(if *degil { sonuc.degil() } else { sonuc })
        }
        Ifade::NullMu { sutun, bekliyor } => {
            let deger = kolon_degeri(sutun, satir, kolonlar)?;
            Ok(if deger.null_mu() == *bekliyor {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            })
        }
        Ifade::Ve(hepler) => {
            let mut toplam = Sonuc::Dogru;
            for hepler in hepler {
                toplam = toplam.ve(degerlendir(hepler, satir, kolonlar)?);
                if toplam == Sonuc::Yanlis {
                    break;
                }
            }
            Ok(toplam)
        }
        Ifade::Ya(hepler) => {
            let mut toplam = Sonuc::Yanlis;
            for hepler in hepler {
                toplam = toplam.veya(degerlendir(hepler, satir, kolonlar)?);
                if toplam == Sonuc::Dogru {
                    break;
                }
            }
            Ok(toplam)
        }
        Ifade::Tumu { ic, .. } => Ok(degerlendir(ic, satir, kolonlar)?.degil()),
    }
}

/// Kolon adını büyük/küçük harf duyarsız biçimde çözer ve hücre değerini döndürür.
fn kolon_degeri(
    sutun: &str,
    satir: &[Deger],
    kolonlar: &KolonHaritasi,
) -> Result<Deger, crate::hata::SahneHata> {
    let indeks = kolonlar
        .get(&sutun.to_ascii_lowercase())
        .copied()
        .ok_or_else(|| crate::hata::SahneHata::KolonYok {
            tablo: "<sorgu>".to_string(),
            kolon: sutun.to_string(),
        })?;
    Ok(satir.get(indeks).cloned().unwrap_or(Deger::Null))
}

/// İki hücre değerini karşılaştırma işleciyle karşılaştırır.
pub fn karsilastir(sol: &Deger, islec: KarsilastirmaIsleci, sag: &Deger) -> Sonuc {
    if sol.null_mu() || sag.null_mu() {
        return Sonuc::Bilinmiyor;
    }
    let sira = sol.karsilastir(sag);
    match islec {
        KarsilastirmaIsleci::Esit => {
            if sira == std::cmp::Ordering::Equal {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
        KarsilastirmaIsleci::EsitDegil => {
            if sira != std::cmp::Ordering::Equal {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
        KarsilastirmaIsleci::Kucuk => {
            if sira == std::cmp::Ordering::Less {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
        KarsilastirmaIsleci::Buyuk => {
            if sira == std::cmp::Ordering::Greater {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
        KarsilastirmaIsleci::KucukEsit => {
            if sira != std::cmp::Ordering::Greater {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
        KarsilastirmaIsleci::BuyukEsit => {
            if sira != std::cmp::Ordering::Less {
                Sonuc::Dogru
            } else {
                Sonuc::Yanlis
            }
        }
    }
}

/// SQLite `LIKE` örüntü eşleştirmesi.
///
/// Jokerler: `%` sıfır veya daha çok karakter, `_` tam bir karakter. Kaçış karakteri
/// yoktur. Büyük/küçük harf duyarsızdır; ASCII dışı karakterlerde `LIKE` ile `GLOB`
/// arasındaki fark bu alt kümede belgelenmiştir.
///
/// Eşleştirme artımlıdır: desenin her adımı için metin konumu yalnızca geri alınabilir
/// şekilde ilerler, böylece `%` içeren desenlerde `O(n·m)` üst sınırı korunur.
pub fn eslesir(metin: &str, desen: &str) -> bool {
    let m: Vec<char> = metin.chars().map(ascii_kucuk).collect();
    let d: Vec<char> = desen.chars().map(ascii_kucuk).collect();
    let mut i = 0usize;
    let mut j = 0usize;
    let mut yildiz = usize::MAX;
    let mut i_geri = 0usize;
    while i < m.len() {
        if j < d.len() && (d[j] == '_' || d[j] == m[i]) {
            i += 1;
            j += 1;
        } else if j < d.len() && d[j] == '%' {
            yildiz = j;
            j += 1;
            i_geri = i;
        } else if yildiz != usize::MAX {
            j = yildiz + 1;
            i_geri += 1;
            i = i_geri;
        } else {
            return false;
        }
    }
    while j < d.len() && d[j] == '%' {
        j += 1;
    }
    j == d.len()
}

/// Yalnızca ASCII harfleri küçük harfe indirir (SQLite'in varsayılan davranışı).
fn ascii_kucuk(c: char) -> char {
    if c.is_ascii_uppercase() {
        c.to_ascii_lowercase()
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::ast::Sabit;

    fn harita(isimler: &[&str]) -> KolonHaritasi {
        isimler
            .iter()
            .enumerate()
            .map(|(i, ad)| (ad.to_ascii_lowercase(), i))
            .collect()
    }

    #[test]
    fn uc_degerli_ve_ve_ya() {
        assert_eq!(Sonuc::Dogru.ve(Sonuc::Yanlis), Sonuc::Yanlis);
        assert_eq!(Sonuc::Dogru.ve(Sonuc::Bilinmiyor), Sonuc::Bilinmiyor);
        assert_eq!(Sonuc::Yanlis.ve(Sonuc::Bilinmiyor), Sonuc::Yanlis);
        assert_eq!(Sonuc::Yanlis.veya(Sonuc::Dogru), Sonuc::Dogru);
        assert_eq!(Sonuc::Bilinmiyor.veya(Sonuc::Dogru), Sonuc::Dogru);
        assert_eq!(Sonuc::Bilinmiyor.veya(Sonuc::Yanlis), Sonuc::Bilinmiyor);
        assert_eq!(Sonuc::Bilinmiyor.degil(), Sonuc::Bilinmiyor);
        assert!(Sonuc::Dogru.dogru_mu());
        assert!(!Sonuc::Bilinmiyor.dogru_mu());
    }

    #[test]
    fn null_karsilastirmasi_bilinmiyor_doner() {
        let sonuc = karsilastir(&Deger::Null, KarsilastirmaIsleci::Esit, &Deger::Tam(1));
        assert_eq!(sonuc, Sonuc::Bilinmiyor);
    }

    #[test]
    fn null_karsilastirmasi_satiri_seçmez() {
        // `a = NULL` hiçbir satırı eşleştirmez; ayrıştırıcı bunu zaten reddeder, burada
        // doğrudan ifade kurarak davranışı sınıyoruz.
        let ifade = Ifade::Karsilastirma {
            sutun: "a".into(),
            islec: KarsilastirmaIsleci::Esit,
            sabit: Sabit::Null,
        };
        let satir = vec![Deger::Null];
        let sonuc = degerlendir(&ifade, &satir, &harita(&["a"])).unwrap_or(Sonuc::Yanlis);
        assert_eq!(sonuc, Sonuc::Bilinmiyor);
        assert!(!sonuc.dogru_mu());
    }

    #[test]
    fn is_null_dogru_calisir() {
        let ifade = Ifade::NullMu {
            sutun: "a".into(),
            bekliyor: true,
        };
        let sonuc = degerlendir(&ifade, &[Deger::Null], &harita(&["a"])).unwrap_or(Sonuc::Yanlis);
        assert!(sonuc.dogru_mu());
    }

    #[test]
    fn is_not_null_dogru_calisir() {
        let ifade = Ifade::NullMu {
            sutun: "a".into(),
            bekliyor: false,
        };
        let sonuc = degerlendir(&ifade, &[Deger::Tam(1)], &harita(&["a"])).unwrap_or(Sonuc::Yanlis);
        assert!(sonuc.dogru_mu());
    }

    #[test]
    fn tum_tipler_karsilastirilir() {
        let k = harita(&["a"]);
        let ifade = |islec| Ifade::Karsilastirma {
            sutun: "a".into(),
            islec,
            sabit: Sabit::Metin("ali".into()),
        };
        assert!(degerlendir(
            &ifade(KarsilastirmaIsleci::Esit),
            &[Deger::Metin("ali".into())],
            &k
        )
        .unwrap_or(Sonuc::Yanlis)
        .dogru_mu());
        assert!(!degerlendir(
            &ifade(KarsilastirmaIsleci::EsitDegil),
            &[Deger::Metin("ali".into())],
            &k
        )
        .unwrap_or(Sonuc::Dogru)
        .dogru_mu());
        assert!(degerlendir(
            &ifade(KarsilastirmaIsleci::Kucuk),
            &[Deger::Metin("ahmet".into())],
            &k
        )
        .unwrap_or(Sonuc::Yanlis)
        .dogru_mu());
        assert!(degerlendir(
            &ifade(KarsilastirmaIsleci::BuyukEsit),
            &[Deger::Metin("b".into())],
            &k
        )
        .unwrap_or(Sonuc::Yanlis)
        .dogru_mu());
    }

    #[test]
    fn sayisal_karsilastirma_karisilir() {
        let k = harita(&["a"]);
        let ifade = |islec| Ifade::Karsilastirma {
            sutun: "a".into(),
            islec,
            sabit: Sabit::Tam(10),
        };
        assert!(degerlendir(
            &ifade(KarsilastirmaIsleci::Kucuk),
            &[Deger::Gercek(9.5)],
            &k
        )
        .unwrap_or(Sonuc::Yanlis)
        .dogru_mu());
        assert!(!degerlendir(
            &ifade(KarsilastirmaIsleci::Buyuk),
            &[Deger::Gercek(9.5)],
            &k
        )
        .unwrap_or(Sonuc::Dogru)
        .dogru_mu());
    }

    #[test]
    fn like_jokerleri() {
        assert!(eslesir("ali", "al%"));
        assert!(eslesir("ali", "%i"));
        assert!(eslesir("ali", "%li%"));
        assert!(eslesir("ali", "a_i"));
        assert!(eslesir("ali", "%"));
        assert!(eslesir("", "%"));
        assert!(eslesir("", ""));
        assert!(!eslesir("ali", "a_"));
        assert!(!eslesir("ali", "ali%a"));
        assert!(eslesir("ab", "a%%b"));
    }

    #[test]
    fn like_buyuk_kucuk_harf_duyarsiz() {
        assert!(eslesir("ALI", "ali"));
        assert!(eslesir("ali", "Al%"));
    }

    #[test]
    fn like_metin_disi_degerde_bilinmiyor() {
        let k = harita(&["a"]);
        let ifade = Ifade::Benzer {
            sutun: "a".into(),
            desen: "1%".into(),
            degil: false,
        };
        let sonuc = degerlendir(&ifade, &[Deger::Tam(123)], &k).unwrap_or(Sonuc::Dogru);
        assert_eq!(sonuc, Sonuc::Bilinmiyor);
    }

    #[test]
    fn not_like_degil_uygular() {
        let k = harita(&["a"]);
        let ifade = Ifade::Benzer {
            sutun: "a".into(),
            desen: "a%".into(),
            degil: true,
        };
        assert!(degerlendir(&ifade, &[Deger::Metin("veli".into())], &k)
            .unwrap_or(Sonuc::Yanlis)
            .dogru_mu());
    }

    #[test]
    fn bilinmeyen_kolon_hata_verir() {
        let ifade = Ifade::NullMu {
            sutun: "yok".into(),
            bekliyor: true,
        };
        assert!(degerlendir(&ifade, &[Deger::Null], &harita(&["a"])).is_err());
    }

    #[test]
    fn ve_ile_kisa_devre() {
        let k = harita(&["a", "b"]);
        let ifade = Ifade::Ve(vec![
            Ifade::Karsilastirma {
                sutun: "a".into(),
                islec: KarsilastirmaIsleci::Esit,
                sabit: Sabit::Tam(1),
            },
            Ifade::Karsilastirma {
                sutun: "b".into(),
                islec: KarsilastirmaIsleci::Esit,
                sabit: Sabit::Tam(2),
            },
        ]);
        assert!(degerlendir(&ifade, &[Deger::Tam(1), Deger::Tam(2)], &k)
            .unwrap_or(Sonuc::Yanlis)
            .dogru_mu());
        assert!(!degerlendir(&ifade, &[Deger::Tam(1), Deger::Tam(3)], &k)
            .unwrap_or(Sonuc::Dogru)
            .dogru_mu());
    }

    #[test]
    fn satir_kisa_veya_uzun_olsay_nil_donuyor() {
        let k = harita(&["a", "b"]);
        let ifade = Ifade::NullMu {
            sutun: "b".into(),
            bekliyor: true,
        };
        assert!(degerlendir(&ifade, &[Deger::Tam(1)], &k)
            .unwrap_or(Sonuc::Yanlis)
            .dogru_mu());
    }
}
