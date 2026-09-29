//! SQLite değişken uzunluklu tam sayı (varint) kodlaması.
//!
//! Kodlama büyük uçlu (big-endian) ve 7 bitlik gruplar hâlindedir. Birinci sekiz bayt
//! en yüksek anlamlı biti 1 olan birer bayttır; dokuzuncu bayt tüm 8 bitini taşır ve
//! yalnızca negatif sayılarda (8 bayttan uzun işaretli değerler) kullanılır.
//!
//! Kaynak: SQLite dosya biçimi belgesi, "Variable-Length Integers" bölümü.

use crate::hata::SahneHata;

/// Maksimum uzunluk: bir varint en fazla dokuz bayttır.
pub const VARINT_EN_FAZLA_BAYT: usize = 9;

/// Bir varint okundu; `sonraki` değer okuma için kullanılacak bayt indeksidir.
///
/// # Hatalar
///
/// Veri beklenenden erken biter veya dokuz bayttan uzun bir kodlama varsa
/// [`SahneHata::BozukSayfa`] hatası döner (okunan bayt bilgisi eklenmez).
pub fn varint_oku(veri: &[u8], baslangic: usize) -> Result<(i64, usize), SahneHata> {
    let mut deger: u64 = 0;
    for i in 0..VARINT_EN_FAZLA_BAYT {
        let konum = baslangic + i;
        let bayt = *veri.get(konum).ok_or_else(|| SahneHata::BozukSayfa {
            sayfa: 0,
            ayrinti: format!(
                "konum {konum} içinde varint bekleniyordu, veri {} bayt",
                veri.len()
            ),
        })?;
        if i == VARINT_EN_FAZLA_BAYT - 1 {
            // Dokuzuncu bayt 8 bitinin tamamını taşır ve terminal kabul edilir:
            // yüksek bit set olsa da kodlama burada biter.
            deger = (deger << 8) | u64::from(bayt);
            return Ok((deger as i64, konum + 1));
        }
        deger = (deger << 7) | u64::from(bayt & 0x7F);
        if bayt & 0x80 == 0 {
            return Ok((deger as i64, konum + 1));
        }
    }
    // Döngü dokuz bayt sınırına ulaştığında yukarıdaki dal zaten dönüş yapar; bu
    // satır yalnızca döngünün `Result` ile düzgün sonlanmasını sağlar (sözleşme §4.2:
    // üretim kodunda `panic!` ailesi yoktur).
    Err(SahneHata::BozukSayfa {
        sayfa: 0,
        ayrinti: format!("{VARINT_EN_FAZLA_BAYT} bayt sınırına ulaşıldı ama varint bitmedi"),
    })
}

/// Bir `i64` değerini SQLite varint biçiminde yazar.
///
/// Negatif değerler her zaman dokuz baytlık biçimde kodlanır; bu SQLite'in kendi
/// yazma yolunun da uyduğu kurallardan biridir.
pub fn varint_yaz(deger: i64) -> Vec<u8> {
    let isaretsiz = deger as u64;
    if deger < 0 {
        return dokuz_baytlik(isaretsiz);
    }

    // Pozitif değerler için gereken bayt sayısı (her bayt 7 bit taşır).
    let mut bayt_sayisi = 1;
    let mut kalan = isaretsiz >> 7;
    while kalan != 0 {
        bayt_sayisi += 1;
        kalan >>= 7;
    }
    if bayt_sayisi >= 9 {
        // 8 bayt 7'şer bit yetmez; dokuzuncu bayt 8 bit taşır.
        return dokuz_baytlik(isaretsiz);
    }

    let mut baytlar = Vec::with_capacity(bayt_sayisi);
    for i in 0..bayt_sayisi {
        let kaydirma = 7 * (bayt_sayisi - 1 - i);
        let mut bayt = ((isaretsiz >> kaydirma) & 0x7F) as u8;
        if i + 1 < bayt_sayisi {
            bayt |= 0x80;
        }
        baytlar.push(bayt);
    }
    baytlar
}

/// Dokuz baytlık biçim: ilk sekiz bayt 7'şer bit (en yüksek bitler başta), dokuzuncu
/// bayt kalan 8 biti taşır. Negatif değerler ve 56 bitten büyük pozitif değerler
/// bu biçimi kullanır.
fn dokuz_baytlik(isaretsiz: u64) -> Vec<u8> {
    let mut baytlar = Vec::with_capacity(9);
    for i in 0..8 {
        let kaydirma = 57 - 7 * i;
        baytlar.push((((isaretsiz >> kaydirma) & 0x7F) as u8) | 0x80);
    }
    baytlar.push((isaretsiz & 0xFF) as u8);
    baytlar
}

/// Varint kodlaması için gereken bayt sayısını döndürür.
pub fn varint_uzunluk(deger: i64) -> usize {
    varint_yaz(deger).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coz(baytlar: &[u8]) -> (i64, usize) {
        match varint_oku(baytlar, 0) {
            Ok(sonuc) => sonuc,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    #[test]
    fn kisa_degerler_tek_bayt() {
        assert_eq!(coz(&[0x00]), (0, 1));
        assert_eq!(coz(&[0x7F]), (127, 1));
    }

    #[test]
    fn sinir_degerleri_ve_geri_donus() {
        for deger in [
            0i64,
            1,
            126,
            127,
            128,
            300,
            16383,
            16384,
            1_000_000,
            i64::MAX,
        ] {
            let baytlar = varint_yaz(deger);
            assert!(baytlar.len() <= VARINT_EN_FAZLA_BAYT);
            assert_eq!(coz(&baytlar), (deger, baytlar.len()), "deger {deger}");
            assert_eq!(varint_uzunluk(deger), baytlar.len());
        }
    }

    #[test]
    fn negatif_degerler_dokuz_bayt() {
        for deger in [-1i64, -2, -1000, i64::MIN] {
            let baytlar = varint_yaz(deger);
            assert_eq!(baytlar.len(), 9, "deger {deger}");
            assert_eq!(coz(&baytlar).0, deger, "deger {deger}");
        }
    }

    #[test]
    fn buyuk_pozitif_degerler_dokuz_bayt() {
        for deger in [1i64 << 56, 1i64 << 62, i64::MAX] {
            let baytlar = varint_yaz(deger);
            assert_eq!(baytlar.len(), 9, "deger {deger}");
            assert_eq!(coz(&baytlar).0, deger, "deger {deger}");
        }
    }

    #[test]
    fn uzun_bayt_dizisinde_konum_ilerler() {
        // 0xFF 0x01 = 0b1111111 << 7 | 0b0000001 = 16257
        let veri = [0xFF, 0x01, 0x41];
        let (deger, sonraki) = match varint_oku(&veri, 0) {
            Ok(sonuc) => sonuc,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(deger, 16257);
        assert_eq!(sonraki, 2);
    }

    #[test]
    fn kesik_varint_hata_verir() {
        let sonuc = varint_oku(&[0x81], 0);
        assert!(sonuc.is_err());
    }

    #[test]
    fn dokuzuncu_bayt_dokuz_bayt_sinirini_kapatir() {
        // SQLite dokuzuncu baytı terminal kabul eder; yüksek bit set olsa da okuma biter.
        let veri = [0x80u8; 9];
        let (deger, sonraki) = coz(&veri);
        assert_eq!(sonraki, 9);
        assert_eq!(deger, 128);
    }

    #[test]
    fn varint_uzunluk_gercek_kodlama_uzunluguna_esit() {
        assert_eq!(varint_uzunluk(0), 1);
        assert_eq!(varint_uzunluk(127), 1);
        assert_eq!(varint_uzunluk(128), 2);
        assert_eq!(varint_uzunluk(-1), 9);
    }

    #[test]
    fn baslangic_disi_konum_dogrulanir() {
        let veri = [0x00, 0x00, 0x05];
        match varint_oku(&veri, 2) {
            Ok((deger, sonraki)) => {
                assert_eq!(deger, 5);
                assert_eq!(sonraki, 3);
            }
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        }
    }

    #[test]
    fn dokuzuncu_bayt_yolu_i64_min_gidis_gelir() {
        // Regresyon: döngü sonu `unreachable!` ile kapatılmıştı. Dokuzuncu bayt yolu
        // artık yalnızca `Result` dönüşüyle sonlanır; `i64::MIN` gidiş-dönüşü bunu
        // doğrular.
        let baytlar = varint_yaz(i64::MIN);
        assert_eq!(baytlar.len(), VARINT_EN_FAZLA_BAYT);
        // İlk sekiz baytın yüksek biti set olmalı, dokuzuncu bayt 8 bit taşımalı.
        assert!(
            baytlar[..8].iter().all(|b| b & 0x80 != 0),
            "ilk sekiz bayt devamlılık biti taşımalı"
        );
        assert_eq!(baytlar[8], 0x00, "dokuzuncu bayt 8 bit taşır");
        assert_eq!(coz(&baytlar), (i64::MIN, VARINT_EN_FAZLA_BAYT));
        assert_eq!(varint_uzunluk(i64::MIN), VARINT_EN_FAZLA_BAYT);
    }

    #[test]
    fn dokuzuncu_bayt_yolu_tam_deger_kumesi() {
        // Dokuzuncu bayta düşen her değer ailesi: negatifler, 56 bitten büyük
        // pozitifler ve sınır değerleri.
        for deger in [
            i64::MIN,
            -1,
            -2_147_483_648,
            -(1i64 << 55),
            -(1i64 << 56),
            1i64 << 56,
            1i64 << 57,
            1i64 << 62,
            i64::MAX,
        ] {
            let baytlar = varint_yaz(deger);
            assert_eq!(
                baytlar.len(),
                VARINT_EN_FAZLA_BAYT,
                "değer {deger} dokuz bayt olmalı"
            );
            assert_eq!(
                coz(&baytlar),
                (deger, VARINT_EN_FAZLA_BAYT),
                "değer {deger}"
            );
        }
    }

    #[test]
    fn dokuz_bayttan_uzun_kodlama_yoktur() {
        // Onuncu bayta taşmayı gerektiren bir dizi yok sayılmalı: dokuzuncu bayt
        // terminaldir ve okuma dokuz baytta biter.
        let veri = [0x80u8; 12];
        let (deger, sonraki) = coz(&veri);
        assert_eq!(sonraki, VARINT_EN_FAZLA_BAYT);
        assert_eq!(deger, 128);
    }
}
