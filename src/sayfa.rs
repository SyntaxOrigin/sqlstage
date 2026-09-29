//! b-tree sayfa başlığı ve hücre dizisi ayrıştırması.
//!
//! SQLite tabloları bir "tablo b-tree" olarak, indeksleri ise bir "indeks b-tree" olarak
//! saklanır. Bu modül yalnızca **tek bir sayfayı** çözer; ağaç gezinmesi
//! [`crate::veritabani`] modülündedir.
//!
//! Kaynak: <https://www.sqlite.org/fileformat2.html#b_tree_pages>

use crate::hata::SahneHata;

/// b-tree sayfa türü (sayfa başlığının ilk baytı).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SayfaTuru {
    /// Tablo b-tree iç sayfası (0x05): sol çocuk sayfa + satır anahtarı.
    TabloIc,
    /// Tablo b-tree yaprak sayfası (0x0D): satır kimliği + kayıt yükü.
    TabloYaprak,
    /// İndeks b-tree iç sayfası (0x02): sol çocuk + indeks kaydı.
    IndeksIc,
    /// İndeks b-tree yaprak sayfası (0x0A): indeks kaydı.
    IndeksYaprak,
}

impl SayfaTuru {
    /// Sayfa başlığı baytından türü üretir.
    ///
    /// # Hatalar
    ///
    /// Tanınmayan bir bayt (örneğin boş bir sayfa veya rastgele veri) hata verir.
    pub fn bayttan(bayt: u8) -> Result<Self, SahneHata> {
        match bayt {
            0x02 => Ok(Self::IndeksIc),
            0x05 => Ok(Self::TabloIc),
            0x0A => Ok(Self::IndeksYaprak),
            0x0D => Ok(Self::TabloYaprak),
            diger => Err(SahneHata::BozukSayfa {
                sayfa: 0,
                ayrinti: format!("tanınmayan b-tree sayfa türü 0x{diger:02X}"),
            }),
        }
    }

    /// Sayfa türünün ham bayt değeri.
    pub fn bayt(&self) -> u8 {
        match self {
            Self::IndeksIc => 0x02,
            Self::TabloIc => 0x05,
            Self::IndeksYaprak => 0x0A,
            Self::TabloYaprak => 0x0D,
        }
    }

    /// Bu sayfa türünün iç sayfa olup olmadığını döndürür.
    pub fn ic_sayfa_mi(&self) -> bool {
        matches!(self, Self::TabloIc | Self::IndeksIc)
    }

    /// Bu sayfa türünde hücre yükünün sayfa içinde tutulan kısmı mı yoksa indeks
    /// kaydı mı olduğunu söyler.
    pub fn indeks_mi(&self) -> bool {
        matches!(self, Self::IndeksIc | Self::IndeksYaprak)
    }

    /// Sayfa başlığının bayt uzunluğu (iç sayfalarda sağ-çocuk işaretçisi eklenir).
    pub fn baslik_uzunlugu(&self) -> usize {
        if self.ic_sayfa_mi() {
            12
        } else {
            8
        }
    }
}

/// Sayfadaki tek bir hücrenin ayrıştırılmış hâli.
///
/// Hücre yükü sayfaya sığmıyorsa yalnızca "yerel" (local) kısım saklanır; kalanı
/// `varisma_sayfasi` işaret ettiği zincirden okunur.
#[derive(Debug, Clone)]
pub struct Hucre {
    /// Hücrenin sayfa içindeki bayt ofseti.
    pub ofset: u16,
    /// İç sayfalarda sol çocuk sayfa numarası.
    pub sol_cocuk: Option<u32>,
    /// Tablo sayfalarında satır kimliği (rowid); indeks sayfalarında `0`.
    pub anahtar: i64,
    /// Bildirilen toplam yük boyutu (bayt).
    pub bildirilen_yuk: i64,
    /// Sayfa içinde saklanan yük parçası.
    pub yerel: Vec<u8>,
    /// Yükün devamının bulunduğu ilk varışma sayfası (yoksa `None`).
    pub varisma_sayfasi: Option<u32>,
}

impl Hucre {
    /// Yükün sayfa dışına taşıp taşmadığını döndürür.
    pub fn varisma_var_mi(&self) -> bool {
        self.varisma_sayfasi.is_some()
    }
}

/// Ayrıştırılmış tek bir b-tree sayfası.
#[derive(Debug, Clone)]
pub struct BTreeSayfasi {
    /// Sayfa türü.
    pub tur: SayfaTuru,
    /// İlk boş blok zincirinin başlangıcı (0 ise boş blok yok).
    pub ilk_bos_blok: u16,
    /// Hucre sayısı.
    pub hucre_sayisi: u16,
    /// Parçalanmış boş bayt sayısı (sağlıklı dosyalarda 0 veya çok küçük).
    pub parcalanmis_bos_bayt: u8,
    /// İç sayfalarda en sağdaki çocuk sayfa numarası.
    pub sag_cocuk: Option<u32>,
    /// Sırayla okunan hücreler (hücre işaretçi dizisindeki sıraya göre).
    pub hucreler: Vec<Hucre>,
}

impl BTreeSayfasi {
    /// Sayfadaki hücrelerin toplam bildirilen yük boyutunu döndürür.
    ///
    /// Bu değer tarama maliyeti tahmininde kullanılır; yükün çözülmesi gerektirmez.
    pub fn bildirilen_yuk_toplami(&self) -> u64 {
        self.hucreler
            .iter()
            .map(|h| h.bildirilen_yuk.max(0) as u64)
            .sum()
    }
}

/// Bir sayfanın b-tree başlığının sayfa içinde başladığı bayt ofseti (sayfa 1'de 100).
pub const BIRINCI_SAYFA_BASLIK_OFSETI: usize = crate::baslik::BASLIK_UZUNLUGU;

/// Bir sayfayı b-tree düzeni olarak ayrıştırır.
///
/// `sayfa` yalnızca kullanılabilir kısmı içermelidir (rezerve alan çıkarılmış olmalıdır).
/// `ofset`, b-tree başlığının sayfa içindeki başlangıç ofsetidir: 1. sayfa için 100,
/// diğer sayfalar için 0.
///
/// # Hatalar
///
/// Sayfa türü tanınmıyorsa, hücre sayısı başlık uzunluğundan taşıyorsa, hücre işaretçisi
/// sayfa dışını gösteriyorsa veya hücre içeriği kullanılabilir alanı aşıyorsa hata döner.
pub fn sayfa_ayikla(
    sayfa: &[u8],
    sayfa_no: u32,
    ofset: usize,
    kullanilabilir: u32,
) -> Result<BTreeSayfasi, SahneHata> {
    let tur = match sayfa.get(ofset) {
        Some(bayt) => SayfaTuru::bayttan(*bayt)?,
        None => {
            return Err(SahneHata::BozukSayfa {
                sayfa: sayfa_no,
                ayrinti: format!("b-tree başlığı {ofset} ofsetinde yok"),
            })
        }
    };
    let baslik_uzunlugu = tur.baslik_uzunlugu();
    let son = ofset
        .checked_add(baslik_uzunlugu)
        .filter(|s| *s <= sayfa.len())
        .ok_or_else(|| SahneHata::BozukSayfa {
            sayfa: sayfa_no,
            ayrinti: "sayfa başlığı sayfa sınırını aşıyor".to_string(),
        })?;

    let ilk_bos_blok = u16::from_be_bytes([sayfa[ofset + 1], sayfa[ofset + 2]]);
    let hucre_sayisi = u16::from_be_bytes([sayfa[ofset + 3], sayfa[ofset + 4]]);
    let parcalanmis = sayfa[ofset + 7];
    let sag_cocuk = if tur.ic_sayfa_mi() {
        Some(u32::from_be_bytes([
            sayfa[ofset + 8],
            sayfa[ofset + 9],
            sayfa[ofset + 10],
            sayfa[ofset + 11],
        ]))
    } else {
        None
    };

    let isaretci_sonu = son + usize::from(hucre_sayisi) * 2;
    if isaretci_sonu > sayfa.len() {
        return Err(SahneHata::BozukSayfa {
            sayfa: sayfa_no,
            ayrinti: format!("{hucre_sayisi} hücre işaretçisi başlık uzunluğunu ({son}) aşıyor"),
        });
    }

    let mut hucreler = Vec::with_capacity(usize::from(hucre_sayisi));
    for indeks in 0..usize::from(hucre_sayisi) {
        let isaretci = son + indeks * 2;
        let ham = u16::from_be_bytes([sayfa[isaretci], sayfa[isaretci + 1]]);
        // Sayfa boyutu 65536 ise içerik başlangıcı alanı 0 ile temsil edilir.
        let hucre_ofseti: usize = if ham == 0 && kullanilabilir == 65536 {
            65536
        } else {
            usize::from(ham)
        };
        hucreler.push(hucre_ayikla(
            sayfa,
            sayfa_no,
            hucre_ofseti,
            tur,
            kullanilabilir,
        )?);
    }

    Ok(BTreeSayfasi {
        tur,
        ilk_bos_blok,
        hucre_sayisi,
        parcalanmis_bos_bayt: parcalanmis,
        sag_cocuk,
        hucreler,
    })
}

/// Tek bir hücrenin içeriğini ayrıştırır.
fn hucre_ayikla(
    sayfa: &[u8],
    sayfa_no: u32,
    ofset: usize,
    tur: SayfaTuru,
    kullanilabilir: u32,
) -> Result<Hucre, SahneHata> {
    let bozuk = |ayrinti: String| SahneHata::BozukSayfa {
        sayfa: sayfa_no,
        ayrinti,
    };
    let alan = &sayfa[..usize::try_from(kullanilabilir)
        .unwrap_or(sayfa.len())
        .min(sayfa.len())];
    if ofset >= sayfa.len() {
        return Err(bozuk(format!("hücre ofseti {ofset} sayfa sınırı dışında")));
    }

    let mut konum = ofset;
    let sol_cocuk = if tur.ic_sayfa_mi() {
        if konum + 4 > sayfa.len() {
            return Err(bozuk(
                "iç sayfa hücresi çocuk işaretçisi için fazla kısa".into(),
            ));
        }
        let cocuk = u32::from_be_bytes([
            sayfa[konum],
            sayfa[konum + 1],
            sayfa[konum + 2],
            sayfa[konum + 3],
        ]);
        konum += 4;
        Some(cocuk)
    } else {
        None
    };

    // Tablo b-tree iç sayfası: çocuk işaretçisinden sonra yalnızca satır anahtarı vardır,
    // yük ve varışma zinciri yoktur.
    if tur == SayfaTuru::TabloIc {
        let (anahtar, _) = crate::varint::varint_oku(alan, konum)
            .map_err(|_| bozuk(format!("hücre {ofset} anahtar varint'ı okunamadı")))?;
        return Ok(Hucre {
            ofset: ofset as u16,
            sol_cocuk,
            anahtar,
            bildirilen_yuk: 0,
            yerel: Vec::new(),
            varisma_sayfasi: None,
        });
    }

    if tur.indeks_mi() {
        let (yuk, sonraki) = crate::varint::varint_oku(alan, konum)
            .map_err(|_| bozuk(format!("hücre {ofset} yük boyutu varint'ı okunamadı")))?;
        konum = sonraki;
        let (yerel_uzunluk, varisma) = yuk_bol(
            usize::try_from(yuk.max(0)).unwrap_or(0),
            tur,
            kullanilabilir,
        );
        if konum + yerel_uzunluk > sayfa.len() {
            return Err(bozuk(format!(
                "hücre {ofset} yerel yükü ({yerel_uzunluk}) sayfa sınırını aşıyor"
            )));
        }
        let yerel = sayfa[konum..konum + yerel_uzunluk].to_vec();
        let varisma_sayfasi = if varisma {
            let bas = konum + yerel_uzunluk;
            if bas + 4 > sayfa.len() {
                return Err(bozuk(format!(
                    "hücre {ofset} varışma işaretçisi sayfa dışında"
                )));
            }
            Some(u32::from_be_bytes([
                sayfa[bas],
                sayfa[bas + 1],
                sayfa[bas + 2],
                sayfa[bas + 3],
            ]))
        } else {
            None
        };
        return Ok(Hucre {
            ofset: ofset as u16,
            sol_cocuk,
            anahtar: 0,
            bildirilen_yuk: yuk,
            yerel,
            varisma_sayfasi,
        });
    }

    // Tablo b-tree hücresi: [sol çocuk] + varint(yük) + varint(satır kimliği) + yük
    let (yuk, sonraki) = crate::varint::varint_oku(alan, konum)
        .map_err(|_| bozuk(format!("hücre {ofset} yük boyutu varint'ı okunamadı")))?;
    konum = sonraki;
    let (satir_kimligi, sonraki) = crate::varint::varint_oku(alan, konum)
        .map_err(|_| bozuk(format!("hücre {ofset} satır kimliği varint'ı okunamadı")))?;
    konum = sonraki;

    let (yerel_uzunluk, varisma) = yuk_bol(
        usize::try_from(yuk.max(0)).unwrap_or(0),
        tur,
        kullanilabilir,
    );
    if konum + yerel_uzunluk > sayfa.len() {
        return Err(bozuk(format!(
            "hücre {ofset} yerel yükü ({yerel_uzunluk}) sayfa sınırını aşıyor"
        )));
    }
    let yerel = sayfa[konum..konum + yerel_uzunluk].to_vec();
    let varisma_sayfasi = if varisma {
        let bas = konum + yerel_uzunluk;
        if bas + 4 > sayfa.len() {
            return Err(bozuk(format!(
                "hücre {ofset} varışma işaretçisi sayfa dışında"
            )));
        }
        Some(u32::from_be_bytes([
            sayfa[bas],
            sayfa[bas + 1],
            sayfa[bas + 2],
            sayfa[bas + 3],
        ]))
    } else {
        None
    };

    Ok(Hucre {
        ofset: ofset as u16,
        sol_cocuk,
        anahtar: satir_kimligi,
        bildirilen_yuk: yuk,
        yerel,
        varisma_sayfasi,
    })
}

/// Bir yükün sayfada tutulan parçasının uzunluğunu ve varışma kullanılıp
/// kullanılmadığını hesaplar.
///
/// SQLite dosya biçimi iki farklı yerel yük formülü tanımlar: tablo b-tree hücreleri
/// `U - 35` üst sınırı, indeks hücreleri ise `((U-12)*64/255)-23` üst sınırı kullanır.
///
/// Kaynak: <https://www.sqlite.org/fileformat2.html#b_tree_pages>
pub fn yuk_bol(toplam: usize, tur: SayfaTuru, kullanilabilir: u32) -> (usize, bool) {
    let u = kullanilabilir as usize;
    let en_cok = if tur.indeks_mi() {
        ((u.saturating_sub(12) * 64) / 255).saturating_sub(23)
    } else {
        u.saturating_sub(35)
    };
    let en_az = ((u.saturating_sub(12) * 32) / 255).saturating_sub(23);
    if toplam <= en_cok {
        return (toplam, false);
    }
    let bolen = u.saturating_sub(4).max(1);
    let k = en_az + (toplam - en_az) % bolen;
    let yerel = if k <= en_cok { k } else { en_az };
    (yerel, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baslik::BASLIK_UZUNLUGU;

    /// 512 baytlık, tek yaprak sayfalı geçerli bir tablo b-tree sayfası üretir.
    fn yaprak_sayfa(hucruler: &[(i64, Vec<u8>)], sayfa_boyutu: u16) -> Vec<u8> {
        let mut sayfa = vec![0u8; sayfa_boyutu as usize];
        let mut icerik = sayfa_boyutu as usize;
        let mut isaretciler = Vec::new();
        for (rowid, yuk) in hucruler.iter() {
            let mut hucre = Vec::new();
            hucre.extend(crate::varint::varint_yaz(yuk.len() as i64));
            hucre.extend(crate::varint::varint_yaz(*rowid));
            hucre.extend(yuk);
            icerik -= hucre.len();
            sayfa[icerik..icerik + hucre.len()].copy_from_slice(&hucre);
            isaretciler.push(icerik as u16);
        }
        sayfa[0] = 0x0D;
        sayfa[3..5].copy_from_slice(&(hucruler.len() as u16).to_be_bytes());
        sayfa[5..7].copy_from_slice(&(icerik as u16).to_be_bytes());
        for (i, ofset) in isaretciler.iter().enumerate() {
            let konum = 8 + i * 2;
            sayfa[konum..konum + 2].copy_from_slice(&ofset.to_be_bytes());
        }
        sayfa
    }

    #[test]
    fn sayfa_turu_karsiligi() {
        assert_eq!(
            SayfaTuru::bayttan(0x0D).unwrap_or(SayfaTuru::TabloYaprak),
            SayfaTuru::TabloYaprak
        );
        assert_eq!(
            SayfaTuru::bayttan(0x05).unwrap_or(SayfaTuru::TabloYaprak),
            SayfaTuru::TabloIc
        );
        assert_eq!(
            SayfaTuru::bayttan(0x0A).unwrap_or(SayfaTuru::TabloYaprak),
            SayfaTuru::IndeksYaprak
        );
        assert_eq!(
            SayfaTuru::bayttan(0x02).unwrap_or(SayfaTuru::TabloYaprak),
            SayfaTuru::IndeksIc
        );
        assert_eq!(SayfaTuru::TabloYaprak.bayt(), 0x0D);
        assert!(SayfaTuru::TabloIc.ic_sayfa_mi());
        assert!(!SayfaTuru::TabloYaprak.ic_sayfa_mi());
        assert!(SayfaTuru::IndeksYaprak.indeks_mi());
        assert_eq!(SayfaTuru::TabloIc.baslik_uzunlugu(), 12);
        assert_eq!(SayfaTuru::TabloYaprak.baslik_uzunlugu(), 8);
    }

    #[test]
    fn taninmayan_sayfa_turu_reddedilir() {
        assert!(SayfaTuru::bayttan(0x00).is_err());
        assert!(SayfaTuru::bayttan(0xFF).is_err());
    }

    #[test]
    fn yaprak_sayfa_hucreleri_okunur() {
        let sayfa = yaprak_sayfa(&[(1, vec![1, 2, 3]), (2, vec![4, 5])], 512);
        let cozulmus = match sayfa_ayikla(&sayfa, 3, 0, 512) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus.hucre_sayisi, 2);
        assert_eq!(cozulmus.tur, SayfaTuru::TabloYaprak);
        assert_eq!(cozulmus.hucreler[0].anahtar, 1);
        assert_eq!(cozulmus.hucreler[0].yerel, vec![1, 2, 3]);
        assert_eq!(cozulmus.hucreler[1].anahtar, 2);
        assert!(!cozulmus.hucreler[0].varisma_var_mi());
        assert_eq!(cozulmus.bildirilen_yuk_toplami(), 5);
        assert!(cozulmus.sag_cocuk.is_none());
    }

    #[test]
    fn bos_yaprak_sayfa_hucre_sayisi_sifir() {
        let sayfa = yaprak_sayfa(&[], 512);
        let cozulmus = match sayfa_ayikla(&sayfa, 2, 0, 512) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus.hucre_sayisi, 0);
        assert!(cozulmus.hucreler.is_empty());
        assert_eq!(cozulmus.bildirilen_yuk_toplami(), 0);
    }

    #[test]
    fn hucre_isaretcisi_sayfa_disiysa_hata_verir() {
        let mut sayfa = yaprak_sayfa(&[(1, vec![1])], 512);
        sayfa[8..10].copy_from_slice(&600u16.to_be_bytes());
        assert!(sayfa_ayikla(&sayfa, 1, 0, 512).is_err());
    }

    #[test]
    fn asiri_buyuk_hucre_sayisi_hata_verir() {
        let mut sayfa = yaprak_sayfa(&[(1, vec![1])], 512);
        sayfa[3..5].copy_from_slice(&4000u16.to_be_bytes());
        assert!(sayfa_ayikla(&sayfa, 1, 0, 512).is_err());
    }

    #[test]
    fn birinci_sayfa_ofseti_yuz_kullanir() {
        let mut sayfa = vec![0u8; 512];
        sayfa[BASLIK_UZUNLUGU] = 0x0D;
        sayfa[BASLIK_UZUNLUGU + 3..BASLIK_UZUNLUGU + 5].copy_from_slice(&0u16.to_be_bytes());
        let cozulmus = match sayfa_ayikla(&sayfa, 1, BASLIK_UZUNLUGU, 512) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus.hucre_sayisi, 0);
    }

    #[test]
    fn ic_sayfa_sag_cocuk_isaretcisini_okur() {
        let mut sayfa = vec![0u8; 512];
        let mut icerik = 512usize;
        let mut isaretciler = Vec::new();
        for cocuk in [2u32, 3] {
            let mut hucre = Vec::new();
            hucre.extend_from_slice(&cocuk.to_be_bytes());
            hucre.extend(crate::varint::varint_yaz(cocuk as i64 * 10));
            icerik -= hucre.len();
            sayfa[icerik..icerik + hucre.len()].copy_from_slice(&hucre);
            isaretciler.push(icerik as u16);
        }
        sayfa[0] = 0x05;
        sayfa[3..5].copy_from_slice(&2u16.to_be_bytes());
        sayfa[5..7].copy_from_slice(&(icerik as u16).to_be_bytes());
        sayfa[8..12].copy_from_slice(&9u32.to_be_bytes());
        for (i, ofset) in isaretciler.iter().enumerate() {
            let konum = 12 + i * 2;
            sayfa[konum..konum + 2].copy_from_slice(&ofset.to_be_bytes());
        }
        let cozulmus = match sayfa_ayikla(&sayfa, 4, 0, 512) {
            Ok(s) => s,
            Err(hata) => panic!("beklenmeyen hata: {hata}"),
        };
        assert_eq!(cozulmus.tur, SayfaTuru::TabloIc);
        assert_eq!(cozulmus.sag_cocuk, Some(9));
        assert_eq!(cozulmus.hucreler[0].sol_cocuk, Some(2));
        assert_eq!(cozulmus.hucreler[0].anahtar, 20);
        assert_eq!(cozulmus.hucreler[1].anahtar, 30);
    }

    #[test]
    fn yuk_bol_kucuk_yukte_varisma_kullanmaz() {
        let (yerel, varisma) = yuk_bol(10, SayfaTuru::TabloYaprak, 4096);
        assert_eq!(yerel, 10);
        assert!(!varisma);
    }

    #[test]
    fn yuk_bol_buyuk_yukte_varisma_kullanir() {
        let (yerel, varisma) = yuk_bol(100_000, SayfaTuru::TabloYaprak, 4096);
        assert!(varisma);
        assert!(yerel < 4096);
        assert!(yerel >= ((4096 - 12) * 32 / 255) - 23);
    }

    #[test]
    fn yuk_bol_indeks_ust_siniri_daha_kucuk() {
        let (tablo_yerel, _) = yuk_bol(3000, SayfaTuru::TabloYaprak, 4096);
        let (indeks_yerel, _) = yuk_bol(3000, SayfaTuru::IndeksYaprak, 4096);
        assert!(indeks_yerel < tablo_yerel);
    }

    #[test]
    fn kucuk_sayfa_boyutunda_yuk_bol_guvenli() {
        // 512 baytlık sayfada en az değerler sıfıra yaklaşır; panic olmamalı.
        let (yerel, varisma) = yuk_bol(2000, SayfaTuru::TabloYaprak, 512);
        assert!(varisma);
        assert!(yerel < 512);
        let (yerel, varisma) = yuk_bol(200, SayfaTuru::TabloYaprak, 512);
        assert!(!varisma);
        assert_eq!(yerel, 200);
    }
}
