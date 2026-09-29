//! SQL alt kümesi ayrıştırıcısı.
//!
//! Aşağı inişli (recursive descent) ayrıştırıcıdır. Alt kümenin dışındaki her yapı
//! **açıkça** reddedilir: yazma komutları [`SahneHata::YazmaReddi`], JOIN/alt sorgu/
//! toplam fonksiyon gibi yapılar [`SahneHata::Desteklenmiyor`] ile döner. Hiçbir
//! yapı "sessizce yorumlanmaz".

use crate::hata::SahneHata;
use crate::sql::ast::{Ifade, KarsilastirmaIsleci, Projeksiyon, Sabit, Secim, Siralama, Sorgu};
use crate::sql::lexer::Jeton;

/// Veritabanını değiştiren veya okuyucunun hiçbir işine yaramayan komutların listesi.
const YAZMA_KOMUTLARI: &[&str] = &[
    "INSERT",
    "UPDATE",
    "DELETE",
    "DROP",
    "ALTER",
    "CREATE",
    "REPLACE",
    "TRUNCATE",
    "ATTACH",
    "DETACH",
    "VACUUM",
    "REINDEX",
    "PRAGMA",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "ANALYZE",
    "GRANT",
    "REVOKE",
    "TRIGGER",
];

/// SQL alt kümesinin desteklemediği, sessizce yorumlanmaması gereken anahtar sözcükler.
const DESTEKLENMEYEN_ANAHTARLAR: &[(&str, &str)] = &[
    ("JOIN", "birleştirme desteklenmiyor"),
    ("INNER", "birleştirme desteklenmiyor"),
    ("LEFT", "birleştirme desteklenmiyor"),
    ("RIGHT", "birleştirme desteklenmiyor"),
    ("FULL", "birleştirme desteklenmiyor"),
    ("CROSS", "birleştirme desteklenmiyor"),
    ("NATURAL", "birleştirme desteklenmiyor"),
    ("ON", "birleştirme koşulu desteklenmiyor"),
    ("UNION", "küme işlemleri desteklenmiyor"),
    ("INTERSECT", "küme işlemleri desteklenmiyor"),
    ("EXCEPT", "küme işlemleri desteklenmiyor"),
    ("GROUP", "gruplandırma desteklenmiyor"),
    ("HAVING", "gruplandırma desteklenmiyor"),
    ("DISTINCT", "gruplandırma desteklenmiyor"),
    ("WITH", "ortak tablo ifadeleri desteklenmiyor"),
    ("INTO", "yazma hedefi belirtilemez"),
    ("VALUES", "satır değerleri desteklenmiyor"),
    ("SET", "güncelleme kümeleri desteklenmiyor"),
    ("USING", "birleştirme desteklenmiyor"),
    ("CAST", "tür dönüşümü desteklenmiyor"),
    ("CASE", "koşul ifadeleri desteklenmiyor"),
    ("EXISTS", "alt sorgular desteklenmiyor"),
    (
        "IN",
        "yalnızca =, !=, <, >, <=, >=, LIKE, IS NULL karşılaştırmaları destekleniyor",
    ),
    (
        "BETWEEN",
        "yalnızca =, !=, <, >, <=, >=, LIKE, IS NULL karşılaştırmaları destekleniyor",
    ),
    (
        "OFFSET",
        "OFFSET yalnızca LIMIT ile birlikte kullanılabilir",
    ),
];

/// Ayrıştırılmış belirteç dizisi üzerinde gezen ayrıştırıcı durumu.
pub struct Ayristirici {
    jetonlar: Vec<Jeton>,
    konum: usize,
    tablo_adi: String,
    /// `tablo.kolon` biçiminde yazılan nitelemeler; tablo adı bilindiğinde doğrulanır.
    nitelemeler: Vec<(String, String)>,
}

/// SQL metnini ayrıştırıp [`Sorgu`] üretir.
///
/// # Hatalar
///
/// Yazma komutları [`SahneHata::YazmaReddi`], alt küme dışı yapılar
/// [`SahneHata::Desteklenmiyor`], sözdizimi hataları [`SahneHata::SorguHatasi`] ile
/// döner.
pub fn ayikla(metin: &str) -> Result<Sorgu, SahneHata> {
    let tarama = crate::sql::lexer::tara(metin)?;
    let ilk = tarama.ilk_anahtar().unwrap_or_default();
    if YAZMA_KOMUTLARI.contains(&ilk.as_str()) {
        return Err(SahneHata::YazmaReddi { komut: ilk.clone() });
    }
    if ilk != "SELECT" {
        if let Some((_, sebep)) = DESTEKLENMEYEN_ANAHTARLAR.iter().find(|(a, _)| *a == ilk) {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: ilk,
                sebep,
            });
        }
        return Err(SahneHata::SorguHatasi {
            mesaj: "sorgu SELECT ile başlamalıdır".to_string(),
            konum: 0,
        });
    }
    let mut ayristirici = Ayristirici {
        jetonlar: tarama.jetonlar,
        konum: 0,
        tablo_adi: String::new(),
        nitelemeler: Vec::new(),
    };
    ayristirici.sorgu(metin)
}

impl Ayristirici {
    /// Sorgunun tamamını ayrıştırır.
    fn sorgu(&mut self, metin: &str) -> Result<Sorgu, SahneHata> {
        self.bekle("SELECT")?;
        let secim = self.secim()?;
        self.bekle("FROM")?;
        let tablo = self.tablo_adi_bekle()?;
        self.tablo_adi = tablo.clone();
        self.nitelemeleri_dogrula()?;

        let filtre = if self.anahtar_mi("WHERE") {
            self.bekle("WHERE")?;
            Some(self.ve_ifadesi()?)
        } else {
            None
        };

        let mut siralama = Vec::new();
        if self.anahtar_mi("ORDER") {
            self.bekle("ORDER")?;
            self.bekle("BY")?;
            siralama = self.siralama_listesi()?;
        }

        let mut limit = None;
        let mut ofset = None;
        if self.anahtar_mi("LIMIT") {
            self.bekle("LIMIT")?;
            limit = Some(self.sayi_maddesi("LIMIT")?);
            if self.anahtar_mi("OFFSET") {
                self.bekle("OFFSET")?;
                ofset = Some(self.sayi_maddesi("OFFSET")?);
            }
        }
        if self.anahtar_mi("OFFSET") {
            return Err(SahneHata::SorguHatasi {
                mesaj: "OFFSET yalnızca LIMIT'ten sonra kullanılabilir".to_string(),
                konum: self.konum,
            });
        }

        self.son_kontrol()?;
        Ok(Sorgu {
            metin: metin.trim().to_string(),
            tablo,
            secim,
            filtre,
            siralama,
            limit,
            ofset,
        })
    }

    /// `*` veya açık kolon listesi ayrıştırır.
    fn secim(&mut self) -> Result<Secim, SahneHata> {
        if self.siradaki_islec() == Some("*") {
            self.konum += 1;
            return Ok(Secim::Yildiz);
        }
        let mut liste = Vec::new();
        loop {
            let sutun = self.tablo_qualifyli_kolon()?;
            let takma_ad = if self.anahtar_mi("AS") {
                self.konum += 1;
                Some(self.kimlik("takma ad")?)
            } else {
                None
            };
            liste.push(Projeksiyon { sutun, takma_ad });
            if self.siradaki_islec() == Some(",") {
                self.konum += 1;
                continue;
            }
            break;
        }
        Ok(Secim::Sutunlar(liste))
    }

    /// `kolon` ya da `tablo.kolon` biçiminde bir kolon adı okur.
    fn tablo_qualifyli_kolon(&mut self) -> Result<String, SahneHata> {
        let ilk = self.kimlik("kolon adı")?;
        if self.siradaki_islec() == Some(".") {
            self.konum += 1;
            if self.siradaki_islec() == Some("*") {
                return Err(SahneHata::Desteklenmiyor {
                    ozellik: "tablo.*".to_string(),
                    sebep: "tam tablo seçimi desteklenmiyor; kolonları açıkça yazın",
                });
            }
            let ikinci = self.kimlik("kolon adı")?;
            self.nitelemeler.push((ilk, ikinci.clone()));
            return Ok(ikinci);
        }
        self.fonksiyon_kontrolu(&ilk)?;
        Ok(ilk)
    }

    /// `tablo.kolon` yazımlarındaki tablo adının `FROM` ile eşleştiğini doğrular.
    fn nitelemeleri_dogrula(&mut self) -> Result<(), SahneHata> {
        for (tablo, sutun) in std::mem::take(&mut self.nitelemeler) {
            if !ilke_ayni(&tablo, &self.tablo_adi) {
                return Err(SahneHata::SorguHatasi {
                    mesaj: format!("'{tablo}.{sutun}' içindeki tablo adı FROM ile eşleşmiyor"),
                    konum: self.konum,
                });
            }
        }
        Ok(())
    }

    /// Bir kimlikten sonra `(` geliyorsa bunun fonksiyon çağrısı olduğunu reddeder.
    fn fonksiyon_kontrolu(&self, ad: &str) -> Result<(), SahneHata> {
        if self.siradaki_islec() == Some("(") {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: format!("fonksiyon çağrısı: {ad}(...)"),
                sebep: "SQL fonksiyonları ve toplam fonksiyonlar desteklenmiyor",
            });
        }
        Ok(())
    }

    /// `ORDER BY` maddelerini ayrıştırır.
    fn siralama_listesi(&mut self) -> Result<Vec<Siralama>, SahneHata> {
        let mut liste = Vec::new();
        loop {
            let sutun = self.tablo_qualifyli_kolon()?;
            let azalan = if self.anahtar_mi("DESC") {
                self.konum += 1;
                true
            } else {
                if self.anahtar_mi("ASC") {
                    self.konum += 1;
                }
                false
            };
            liste.push(Siralama { sutun, azalan });
            if self.siradaki_islec() == Some(",") {
                self.konum += 1;
                continue;
            }
            break;
        }
        Ok(liste)
    }

    /// `AND` bağlacıyla birleştirilmiş ifadeleri ayrıştırır.
    fn ve_ifadesi(&mut self) -> Result<Ifade, SahneHata> {
        let mut parcalar = vec![self.ya_ifadesi()?];
        while self.anahtar_mi("AND") {
            self.konum += 1;
            parcalar.push(self.ya_ifadesi()?);
        }
        if parcalar.len() == 1 {
            Ok(parcalar.swap_remove(0))
        } else {
            Ok(Ifade::Ve(parcalar))
        }
    }

    /// `OR` bağlacıyla birleştirilmiş ifadeleri ayrıştırır.
    fn ya_ifadesi(&mut self) -> Result<Ifade, SahneHata> {
        let mut parcalar = vec![self.birincil_ifade()?];
        while self.anahtar_mi("OR") {
            self.konum += 1;
            parcalar.push(self.birincil_ifade()?);
        }
        if parcalar.len() == 1 {
            Ok(parcalar.swap_remove(0))
        } else {
            Ok(Ifade::Ya(parcalar))
        }
    }

    /// `NOT`, parantez veya karşılaştırma ifadesi.
    fn birincil_ifade(&mut self) -> Result<Ifade, SahneHata> {
        if self.anahtar_mi("NOT") {
            self.konum += 1;
            let ic = self.birincil_ifade()?;
            return Ok(Ifade::Tumu {
                ic: Box::new(ic),
                degil: true,
            });
        }
        if self.siradaki_islec() == Some("(") {
            self.konum += 1;
            let ic = self.ve_ifadesi()?;
            self.bekle_islec(")")?;
            return Ok(ic);
        }
        self.karsilastirma_ifadesi()
    }

    /// `kolon <işleç> sabit`, `LIKE`, `IS NULL` biçimlerini ayrıştırır.
    fn karsilastirma_ifadesi(&mut self) -> Result<Ifade, SahneHata> {
        let sutun = self.tablo_qualifyli_kolon()?;
        // Karşılaştırma işleci gelmeden önce yasaklı bir anahtar sözcük gelirse
        // (IN, BETWEEN, EXISTS ...) sessizce geçilmez, açıkça reddedilir.
        self.desteklenmeyen_kontrol()?;

        if self.anahtar_mi("IS") {
            self.konum += 1;
            let degil = self.anahtar_mi("NOT");
            if degil {
                self.konum += 1;
            }
            self.bekle("NULL")?;
            return Ok(Ifade::NullMu {
                sutun,
                bekliyor: !degil,
            });
        }

        if self.anahtar_mi("LIKE") {
            self.konum += 1;
            let desen = self.metin_sabiti("LIKE deseni")?;
            return Ok(Ifade::Benzer {
                sutun,
                desen,
                degil: false,
            });
        }
        if self.anahtar_mi("NOT") {
            self.konum += 1;
            self.bekle("LIKE")?;
            let desen = self.metin_sabiti("LIKE deseni")?;
            return Ok(Ifade::Benzer {
                sutun,
                desen,
                degil: true,
            });
        }

        let islec = self.karsilastirma_isleci()?;
        if matches!(
            islec,
            KarsilastirmaIsleci::Esit | KarsilastirmaIsleci::EsitDegil
        ) && self.anahtar_mi("NULL")
        {
            return Err(SahneHata::SorguHatasi {
                mesaj: "NULL ile karşılaştırmak için `IS NULL` kullanın; `= NULL` hiçbir satırı eşleştirmez"
                    .to_string(),
                konum: self.konum,
            });
        }
        let sabit = self.sabit()?;
        Ok(Ifade::Karsilastirma {
            sutun,
            islec,
            sabit,
        })
    }

    /// Sıradaki belirteç desteklenmeyen bir anahtar sözcükse hata döndürür.
    fn desteklenmeyen_kontrol(&self) -> Result<(), SahneHata> {
        if let Some(Jeton::Kimlik(k)) = self.siradaki_jeton() {
            let ust = k.to_ascii_uppercase();
            if let Some((_, sebep)) = DESTEKLENMEYEN_ANAHTARLAR.iter().find(|(a, _)| *a == ust) {
                return Err(SahneHata::Desteklenmiyor {
                    ozellik: ust,
                    sebep,
                });
            }
        }
        Ok(())
    }

    fn karsilastirma_isleci(&mut self) -> Result<KarsilastirmaIsleci, SahneHata> {
        let islec = match self.siradaki_islec() {
            Some("=") | Some("==") => KarsilastirmaIsleci::Esit,
            Some("!=") | Some("<>") => KarsilastirmaIsleci::EsitDegil,
            Some("<") => KarsilastirmaIsleci::Kucuk,
            Some(">") => KarsilastirmaIsleci::Buyuk,
            Some("<=") => KarsilastirmaIsleci::KucukEsit,
            Some(">=") => KarsilastirmaIsleci::BuyukEsit,
            diger => {
                return Err(SahneHata::SorguHatasi {
                    mesaj: format!(
                        "beklenen karşılaştırma işleciydi, bulunan: {}",
                        diger.unwrap_or("sorgu sonu")
                    ),
                    konum: self.konum,
                })
            }
        };
        self.konum += 1;
        Ok(islec)
    }

    /// `WHERE` sağ tarafındaki sabitleri ayrıştırır.
    fn sabit(&mut self) -> Result<Sabit, SahneHata> {
        // `siradaki_jeton()` ödünç alıntısı `self.konum` atamasıyla çakıştığı için
        // önce jeton kopyalanır.
        let jeton = self.siradaki_jeton().cloned();
        match jeton {
            Some(Jeton::TamSayi(i)) => {
                self.konum += 1;
                Ok(Sabit::Tam(i))
            }
            Some(Jeton::KayanSayi(f)) => {
                self.konum += 1;
                Ok(Sabit::Kayan(f))
            }
            Some(Jeton::Metin(s)) => {
                self.konum += 1;
                Ok(Sabit::Metin(s))
            }
            Some(Jeton::Kimlik(k)) => {
                let ust = k.to_ascii_uppercase();
                if ust == "NULL" {
                    self.konum += 1;
                    return Ok(Sabit::Null);
                }
                if let Some((_, sebep)) = DESTEKLENMEYEN_ANAHTARLAR.iter().find(|(a, _)| *a == ust)
                {
                    return Err(SahneHata::Desteklenmiyor {
                        ozellik: ust,
                        sebep,
                    });
                }
                self.fonksiyon_kontrolu(&k)?;
                Err(SahneHata::SorguHatasi {
                    mesaj: format!("sabit değil, sütun adı: {k}"),
                    konum: self.konum,
                })
            }
            diger => Err(SahneHata::SorguHatasi {
                mesaj: format!(
                    "beklenen sabit değil, bulunan: {}",
                    diger
                        .map(|j| format!("{j:?}"))
                        .unwrap_or_else(|| "sorgu sonu".to_string())
                ),
                konum: self.konum,
            }),
        }
    }

    fn metin_sabiti(&mut self, beklenen: &str) -> Result<String, SahneHata> {
        match self.siradaki_jeton() {
            Some(Jeton::Metin(s)) => {
                let deger = s.clone();
                self.konum += 1;
                Ok(deger)
            }
            _ => Err(SahneHata::SorguHatasi {
                mesaj: format!("{beklenen} tek tırnaklı metin olmalıdır"),
                konum: self.konum,
            }),
        }
    }

    fn sayi_maddesi(&mut self, ad: &str) -> Result<usize, SahneHata> {
        match self.siradaki_jeton() {
            Some(Jeton::TamSayi(i)) if *i >= 0 => {
                let deger = *i as usize;
                self.konum += 1;
                Ok(deger)
            }
            _ => Err(SahneHata::SorguHatasi {
                mesaj: format!("{ad} sonrasında sıfır veya daha büyük bir tamsayı bekleniyordu"),
                konum: self.konum,
            }),
        }
    }

    /// `FROM` ardından gelen tek tablo adını okur.
    fn tablo_adi_bekle(&mut self) -> Result<String, SahneHata> {
        if self.siradaki_islec() == Some("(") {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: "türetilmiş tablo".to_string(),
                sebep: "alt sorgu tablo kaynağı olarak desteklenmiyor",
            });
        }
        let ad = self.kimlik("tablo adı")?;
        self.fonksiyon_kontrolu(&ad)?;
        if self.anahtar_mi("AS") {
            return Err(SahneHata::Desteklenmiyor {
                ozellik: "tablo takma adı".to_string(),
                sebep: "takma adlı tablo kaynakları desteklenmiyor",
            });
        }
        Ok(ad)
    }

    fn kimlik(&mut self, beklenen: &str) -> Result<String, SahneHata> {
        match self.siradaki_jeton() {
            Some(Jeton::Kimlik(k)) => {
                let ad = k.clone();
                self.konum += 1;
                Ok(ad)
            }
            diger => Err(SahneHata::SorguHatasi {
                mesaj: format!(
                    "{beklenen} bekleniyordu, bulunan: {}",
                    diger
                        .map(|j| format!("{j:?}"))
                        .unwrap_or_else(|| "sorgu sonu".to_string())
                ),
                konum: self.konum,
            }),
        }
    }

    /// Belirtilen anahtar sözcüğün beklenildiği yeri tüketir.
    fn bekle(&mut self, beklenen: &str) -> Result<(), SahneHata> {
        match self.siradaki_anahtar().as_deref() {
            Some(ust) if ilke_ayni(ust, beklenen) => {
                self.konum += 1;
                Ok(())
            }
            diger => Err(SahneHata::SorguHatasi {
                mesaj: format!(
                    "{beklenen} bekleniyordu, bulunan: {}",
                    diger.unwrap_or("sorgu sonu")
                ),
                konum: self.konum,
            }),
        }
    }

    fn bekle_islec(&mut self, beklenen: &'static str) -> Result<(), SahneHata> {
        if self.siradaki_islec() == Some(beklenen) {
            self.konum += 1;
            return Ok(());
        }
        Err(SahneHata::SorguHatasi {
            mesaj: format!("{beklenen} bekleniyordu"),
            konum: self.konum,
        })
    }

    /// Sorgunun sonunda yalnızca noktalı virgül veya hiçbir şey kaldığını doğrular.
    fn son_kontrol(&mut self) -> Result<(), SahneHata> {
        while self.siradaki_islec() == Some(";") {
            self.konum += 1;
        }
        if let Some(Jeton::Kimlik(k)) = self.siradaki_jeton() {
            let ust = k.to_ascii_uppercase();
            if let Some((_, sebep)) = DESTEKLENMEYEN_ANAHTARLAR.iter().find(|(a, _)| *a == ust) {
                return Err(SahneHata::Desteklenmiyor {
                    ozellik: ust,
                    sebep,
                });
            }
        }
        if self.konum < self.jetonlar.len() {
            return Err(SahneHata::SorguHatasi {
                mesaj: "sorgu sonundan sonra fazladan ifade var".to_string(),
                konum: self.konum,
            });
        }
        Ok(())
    }

    fn siradaki_jeton(&self) -> Option<&Jeton> {
        self.jetonlar.get(self.konum)
    }

    fn siradaki_anahtar(&self) -> Option<String> {
        match self.siradaki_jeton() {
            Some(Jeton::Kimlik(k)) => Some(k.to_ascii_uppercase()),
            _ => None,
        }
    }

    /// Sıradaki belirteç verilen anahtar sözcükle eşleşiyor mu.
    ///
    /// SQL anahtar sözcükleri büyük/küçük harf duyarsızdır; `==` karşılaştırması yerine
    /// bu yardımcı kullanılır çünkü `siradaki_anahtar()` `Option<String>` döndürür.
    fn anahtar_mi(&self, beklenen: &str) -> bool {
        self.siradaki_anahtar()
            .is_some_and(|k| k.eq_ignore_ascii_case(beklenen))
    }

    fn siradaki_islec(&self) -> Option<&'static str> {
        match self.siradaki_jeton() {
            Some(Jeton::Islec(i)) => Some(i),
            _ => None,
        }
    }
}

/// SQL anahtar sözcükleri büyük/küçük harf duyarsızdır.
fn ilke_ayni(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ayikla_test(metin: &str) -> Sorgu {
        match ayikla(metin) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    #[test]
    fn en_basit_sorgu_ayristirilir() {
        let s = ayikla_test("SELECT * FROM kisi");
        assert_eq!(s.tablo, "kisi");
        assert_eq!(s.secim, Secim::Yildiz);
        assert!(s.filtre.is_none());
        assert!(s.siralama.is_empty());
        assert!(s.limit.is_none());
    }

    #[test]
    fn tek_kolonlu_secim_ayristirilir() {
        let s = ayikla_test("SELECT id FROM kisi");
        assert_eq!(
            s.secim,
            Secim::Sutunlar(vec![Projeksiyon {
                sutun: "id".into(),
                takma_ad: None,
            }])
        );
    }

    #[test]
    fn acik_kolon_listesi_ve_takma_ad() {
        let s = ayikla_test("SELECT ad AS isim, yas FROM kisi");
        match &s.secim {
            Secim::Sutunlar(l) => {
                assert_eq!(l.len(), 2);
                assert_eq!(l[0].sutun, "ad");
                assert_eq!(l[0].baslik(), "isim");
                assert_eq!(l[1].baslik(), "yas");
            }
            diger => panic!("beklenmeyen seçim: {diger:?}"),
        }
    }

    #[test]
    fn nitelenmis_kolon_dogrulanir() {
        let s = ayikla_test("SELECT kisi.ad FROM kisi");
        match &s.secim {
            Secim::Sutunlar(l) => assert_eq!(l[0].sutun, "ad"),
            diger => panic!("beklenmeyen seçim: {diger:?}"),
        }
    }

    #[test]
    fn yanlis_niteleme_reddedilir() {
        assert!(ayikla("SELECT diger.ad FROM kisi").is_err());
    }

    #[test]
    fn filtre_ve_limit_ayristirilir() {
        let s = ayikla_test("SELECT * FROM t WHERE yas > 30 LIMIT 5 OFFSET 2");
        assert!(s.filtre.is_some());
        assert_eq!(s.limit, Some(5));
        assert_eq!(s.ofset, Some(2));
        assert_eq!(s.ozet(), "LIMIT 5 OFFSET 2");
    }

    #[test]
    fn siralama_ayristirilir() {
        let s = ayikla_test("SELECT * FROM t ORDER BY ad DESC, yas");
        assert_eq!(s.siralama.len(), 2);
        assert!(s.siralama[0].azalan);
        assert!(!s.siralama[1].azalan);
    }

    #[test]
    fn noktali_virgul_izinlidir() {
        let s = ayikla_test("SELECT * FROM t;");
        assert_eq!(s.tablo, "t");
    }

    #[test]
    fn yazma_komutlari_reddedilir() {
        for komut in [
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "DROP TABLE t",
            "ATTACH DATABASE 'x.db' AS x",
            "INSERT INTO t VALUES (1)",
            "CREATE TABLE x (a INT)",
            "PRAGMA journal_mode",
            "VACUUM",
        ] {
            let sonuc = ayikla(komut);
            assert!(
                matches!(sonuc, Err(SahneHata::YazmaReddi { .. })),
                "{komut} reddedilmedi: {:?}",
                sonuc.err()
            );
        }
    }

    #[test]
    fn desteklenmeyen_yapilar_reddedilir() {
        for sorgu in [
            "SELECT * FROM a JOIN b ON a.id = b.id",
            "SELECT * FROM a LEFT JOIN b ON a.id = b.id",
            "SELECT * FROM a UNION SELECT * FROM b",
            "SELECT * FROM a GROUP BY x",
            "SELECT COUNT(*) FROM a",
            "SELECT * FROM a WHERE id IN (1,2)",
            "SELECT * FROM a WHERE id BETWEEN 1 AND 2",
            "SELECT * FROM a WHERE EXISTS (SELECT 1 FROM b)",
            "SELECT * FROM (SELECT 1)",
            "WITH x AS (SELECT 1) SELECT * FROM x",
        ] {
            let sonuc = ayikla(sorgu);
            assert!(
                matches!(sonuc, Err(SahneHata::Desteklenmiyor { .. })),
                "{sorgu} desteklenmiyor olarak reddedilmedi: {:?}",
                sonuc.err()
            );
        }
    }

    #[test]
    fn esit_null_yerine_is_null_zorunlu() {
        let sonuc = ayikla("SELECT * FROM t WHERE a = NULL");
        match sonuc {
            Err(SahneHata::SorguHatasi { mesaj, .. }) => {
                assert!(mesaj.contains("IS NULL"), "{mesaj}");
            }
            diger => panic!("beklenmeyen sonuç: {diger:?}"),
        }
    }

    #[test]
    fn like_deseni_metin_olmak_zorunda() {
        assert!(ayikla("SELECT * FROM t WHERE a LIKE 5").is_err());
    }

    #[test]
    fn limit_sonrasi_offset_syntax_hatasi() {
        assert!(ayikla("SELECT * FROM t OFFSET 2").is_err());
    }

    #[test]
    fn negatif_limit_reddedilir() {
        assert!(ayikla("SELECT * FROM t LIMIT -1").is_err());
    }

    #[test]
    fn select_disi_ilk_kelime_reddedilir() {
        let sonuc = ayikla("SHOW TABLES");
        assert!(matches!(sonuc, Err(SahneHata::SorguHatasi { .. })));
    }

    #[test]
    fn and_or_parantez_harf_karisimi() {
        let s = ayikla_test("SELECT * FROM t WHERE (a = 1 OR b = 2) AND NOT c = 3");
        let filtre = s.filtre.unwrap_or_else(|| panic!("filtre yok"));
        match filtre {
            Ifade::Ve(hepler) => {
                assert_eq!(hepler.len(), 2);
                assert!(matches!(hepler[1], Ifade::Tumu { .. }));
            }
            diger => panic!("beklenmeyen ifade: {diger:?}"),
        }
    }

    #[test]
    fn karsilastirma_sayaci_dogru() {
        let s = ayikla_test("SELECT * FROM t WHERE a = 1 AND (b = 2 OR c IS NULL)");
        let filtre = s.filtre.unwrap_or_else(|| panic!("filtre yok"));
        assert_eq!(filtre.karsilastirma_sayisi(), 3);
    }

    #[test]
    fn not_like_ayristirilir() {
        let s = ayikla_test("SELECT * FROM t WHERE ad NOT LIKE 'A%'");
        let filtre = s.filtre.unwrap_or_else(|| panic!("filtre yok"));
        match filtre {
            Ifade::Benzer { desen, degil, .. } => {
                assert_eq!(desen, "A%");
                assert!(degil);
            }
            diger => panic!("beklenmeyen ifade: {diger:?}"),
        }
    }

    #[test]
    fn tablo_takma_adi_reddedilir() {
        assert!(matches!(
            ayikla("SELECT * FROM t AS x"),
            Err(SahneHata::Desteklenmiyor { .. })
        ));
    }

    #[test]
    fn tablo_yildizi_reddedilir() {
        assert!(matches!(
            ayikla("SELECT t.* FROM t"),
            Err(SahneHata::Desteklenmiyor { .. })
        ));
    }

    #[test]
    fn from_eksikse_hata_verir() {
        assert!(ayikla("SELECT id").is_err());
    }

    #[test]
    fn kucuk_harf_anahtar_kabul_edilir() {
        let s = ayikla_test("select id from t where a = 1 limit 3");
        assert_eq!(s.tablo, "t");
        assert_eq!(s.limit, Some(3));
    }

    #[test]
    fn fazladan_ifade_reddedilir() {
        assert!(ayikla("SELECT * FROM t LIMIT 2 bir").is_err());
    }

    #[test]
    fn where_sonrasi_where_reddedilir() {
        assert!(ayikla("SELECT * FROM t WHERE a = 1 WHERE b = 2").is_err());
    }
}
