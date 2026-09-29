//! Terminal sonuç ızgarası.
//!
//! Yalnızca ASCII karakterler kullanılır (`+`, `-`, `|`) ve kolonlar sığmadığında
//! kısaltılır: kısaltılan hücrenin sonuna `~` konur. Bu, kütüphane bağımlılığı olmadan
//! okunabilir bir tablo üretmenin en küçük yoludur.

use crate::sql::yurutucu::SorguSonucu;

/// Izgara çizim ayarları.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IzgaraAyari {
    /// Bir kolonun alabileceği en fazla karakter sayısı.
    pub kolon_eni: usize,
    /// Başlık satırı ile veri arasına ayraç çizilip çizilmeyeceği (daima çizilir).
    pub ayiracli: bool,
}

impl Default for IzgaraAyari {
    fn default() -> Self {
        Self {
            kolon_eni: 28,
            ayiracli: true,
        }
    }
}

impl IzgaraAyari {
    /// Verilen kolon eniyle yeni bir ayar üretir.
    pub fn yeni(kolon_eni: usize) -> Self {
        Self {
            kolon_eni,
            ayiracli: true,
        }
    }
}

/// Sonuç kümesini terminal ızgarası biçimine çevirir.
///
/// Sıfır satırlı sonuçta yalnızca başlık ve ayraç satırı üretilir; bu, "sonuç boş" ile
/// "sorgu çalışmadı" ayrımını görünür kılar.
pub fn ciz(sonuc: &SorguSonucu, ayar: &IzgaraAyari) -> String {
    let basliklar: Vec<String> = sonuc
        .sutunlar
        .iter()
        .map(|b| format!("{} ({})", b.ad, b.tur))
        .collect();
    let satirlar: Vec<Vec<String>> = sonuc
        .satirlar
        .iter()
        .map(|satir| satir.iter().map(deger_gosterim::goster).collect())
        .collect();
    ciz_satirlar(&basliklar, &satirlar, ayar.kolon_eni)
}

/// Başlık ve satır listesinden ızgara metni üretir.
pub fn ciz_satirlar(basliklar: &[String], satirlar: &[Vec<String>], kolon_eni: usize) -> String {
    let sutun = basliklar
        .len()
        .max(satirlar.iter().map(|s| s.len()).max().unwrap_or(0));
    if sutun == 0 {
        return "(sonuçta kolon yok)".to_string();
    }
    let en = kolon_eni.max(4);
    let mut genislikler = vec![0usize; sutun];
    for (i, baslik) in basliklar.iter().enumerate() {
        genislikler[i] = genislikler[i].max(kisalt(baslik, en).chars().count());
    }
    for satir in satirlar {
        for (i, hucre) in satir.iter().enumerate() {
            genislikler[i] = genislikler[i].max(kisalt(hucre, en).chars().count());
        }
    }

    let mut cikti = String::new();
    cikti.push_str(&kenar(&genislikler));
    if !basliklar.is_empty() {
        cikti.push_str(&satir_yap(&genislikler, basliklar, en));
    }
    cikti.push_str(&kenar(&genislikler));
    for satir in satirlar {
        cikti.push_str(&satir_yap(&genislikler, satir, en));
    }
    cikti.push_str(&kenar(&genislikler));
    cikti
}

/// Hücre metnini en fazla `en` karaktere indirir, kesilen yerin sonuna `~` koyar.
pub fn kisalt(metin: &str, en: usize) -> String {
    if metin.chars().count() <= en {
        return metin.to_string();
    }
    if en <= 1 {
        return "~".to_string();
    }
    let alinan: String = metin.chars().take(en - 1).collect();
    format!("{alinan}~")
}

fn kenar(genislikler: &[usize]) -> String {
    let mut satir = String::from("+");
    for g in genislikler {
        for _ in 0..(*g + 2) {
            satir.push('-');
        }
        satir.push('+');
    }
    satir.push('\n');
    satir
}

fn satir_yap(genislikler: &[usize], hucreler: &[String], en: usize) -> String {
    let mut satir = String::from("|");
    for (i, g) in genislikler.iter().enumerate() {
        let ham = hucreler.get(i).map(String::as_str).unwrap_or("");
        let kisa = kisalt(ham, en);
        let dolgu = g - kisa.chars().count();
        satir.push(' ');
        satir.push_str(&kisa);
        for _ in 0..dolgu {
            satir.push(' ');
        }
        satir.push_str(" |");
    }
    satir.push('\n');
    satir
}

/// `Deger` gösterimi için küçük yardımcı (döngüsel bağımlılığı önlemek için burada).
mod deger_gosterim {
    use crate::deger::Deger;

    pub fn goster(deger: &Deger) -> String {
        deger.gosterim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deger::Deger;
    use crate::sql::yurutucu::SutunBasligi;

    fn sonuc(basliklar: &[(&str, &str)], satirlar: Vec<Vec<Deger>>) -> SorguSonucu {
        let adet = satirlar.len() as u64;
        SorguSonucu {
            sutunlar: basliklar
                .iter()
                .map(|(ad, tur)| SutunBasligi {
                    ad: (*ad).to_string(),
                    tur: (*tur).to_string(),
                })
                .collect(),
            satirlar,
            taranan_satir: adet,
            eslesen_satir: adet,
            sure_ns: 0,
        }
    }

    #[test]
    fn kisa_metin_kisaltilmaz() {
        assert_eq!(kisalt("abc", 5), "abc");
        assert_eq!(kisalt("abcde", 5), "abcde");
    }

    #[test]
    fn uzun_metin_tilde_ile_kisaltilir() {
        assert_eq!(kisalt("abcdefghij", 5), "abcd~");
        assert_eq!(kisalt("abcdef", 1), "~");
    }

    #[test]
    fn cizilen_izgara_tum_satirlari_icerir() {
        let s = sonuc(
            &[("id", "INTEGER"), ("ad", "TEXT")],
            vec![
                vec![Deger::Tam(1), Deger::Metin("Ali".into())],
                vec![Deger::Tam(2), Deger::Metin("Veli".into())],
            ],
        );
        let metin = ciz(&s, &IzgaraAyari::default());
        assert!(metin.contains("id (INTEGER)"));
        assert!(metin.contains("Ali"));
        assert!(metin.contains("Veli"));
        assert_eq!(metin.lines().filter(|l| l.starts_with('+')).count(), 3);
    }

    #[test]
    fn sigmayan_kolon_kisaltilir_ve_tilde_isaretlenir() {
        let uzun = "x".repeat(200);
        let s = sonuc(&[("metin", "TEXT")], vec![vec![Deger::Metin(uzun)]]);
        let metin = ciz(&s, &IzgaraAyari::yeni(10));
        assert!(metin.contains('~'));
        for satir in metin.lines() {
            assert!(satir.chars().count() <= 14, "satır çok uzun: {satir}");
        }
    }

    #[test]
    fn null_ve_blob_gosterimi() {
        let s = sonuc(
            &[("a", "INTEGER"), ("b", "BLOB")],
            vec![vec![Deger::Null, Deger::Blob(vec![0xAB])]],
        );
        let metin = ciz(&s, &IzgaraAyari::default());
        assert!(metin.contains("NULL"));
        assert!(metin.contains("x'ab'"));
    }

    #[test]
    fn bos_sonuc_yine_cerceve_cizer() {
        let s = sonuc(&[("id", "INTEGER")], Vec::new());
        let metin = ciz(&s, &IzgaraAyari::default());
        assert!(metin.contains("id (INTEGER)"));
        assert_eq!(metin.lines().filter(|l| l.starts_with('+')).count(), 3);
    }
    #[test]
    fn kolonsuz_sonuc_mesaj_dondurur() {
        let s = SorguSonucu {
            sutunlar: Vec::new(),
            satirlar: Vec::new(),
            taranan_satir: 0,
            eslesen_satir: 0,
            sure_ns: 0,
        };
        assert_eq!(ciz(&s, &IzgaraAyari::default()), "(sonuçta kolon yok)");
    }

    #[test]
    fn turkce_karakterler_olusabildirilir() {
        let s = sonuc(
            &[("şehir", "TEXT")],
            vec![vec![Deger::Metin("İzmir".into())]],
        );
        let metin = ciz(&s, &IzgaraAyari::default());
        assert!(metin.contains("İzmir"));
    }
}
