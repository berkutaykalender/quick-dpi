pub type T = (&'static str, &'static str);

pub const TAGLINE: T = ("Güvenmediğiniz ağlar için ek koruma katmanı", "An extra layer of protection for networks you don't trust");

pub const TERMS_TITLE: T = ("QuickDPI'ye hoş geldiniz", "Welcome to QuickDPI");

pub const TERMS_INTRO: T = (
    "QuickDPI, bağlantınızı ağdaki izlemeye ve sahte DNS cevaplarına karşı korumak için geliştirilmiş açık kaynaklı bir ağ güvenliği aracıdır. Devam etmeden önce lütfen okuyun:",
    "QuickDPI is an open-source network security tool that protects your connection against on-network monitoring and forged DNS answers. Please read before you continue:",
);

pub const TERMS: &[T] = &[
    ("Yazılım \"olduğu gibi\", hiçbir garanti verilmeden sunulur.", "The software is provided \"as is\", without warranty of any kind."),
        (
            "Yalnızca güvenmediğiniz ağlarda (ortak Wi-Fi, otel, kafe ve havalimanı ağları gibi) kullanılmak üzere tasarlanmıştır. Bir kurumun ağında kullanmadan önce ağ yöneticisinin iznini alın.",
            "It is designed for use only on networks you don't trust (such as public Wi-Fi, hotel, café and airport networks). Before using it on an organisation's network, get the network administrator's permission.",
        ),
    (
        "Kullanımından doğan tüm sorumluluk kullanıcıya aittir. Bulunduğunuz yerin yasalarına, kurumunuzun ve ağ yöneticinizin kurallarına uymak sizin sorumluluğunuzdadır.",
        "You are solely responsible for how you use it. Complying with the laws where you are and with the rules of your organisation and network administrator is your responsibility.",
    ),
    (
        "Geliştiriciler, yazılımın kullanımından ya da kullanılamamasından doğabilecek doğrudan veya dolaylı hiçbir zarardan sorumlu tutulamaz.",
        "The developers are not liable for any direct or indirect damage arising from the use of, or inability to use, the software.",
    ),
    ("QuickDPI bir VPN değildir: trafiğinizi şifrelemez ve kimliğinizi gizlemez.", "QuickDPI is not a VPN: it does not encrypt your traffic or hide your identity."),
    ("Kişisel veri toplamaz, hesap istemez.", "It collects no personal data and needs no account."),
];

pub const TERMS_CHECK: T = ("Okudum, anladım ve kabul ediyorum", "I have read, understood and accept these terms");
pub const TERMS_ACCEPT: T = ("Kabul et ve devam et", "Accept and continue");
pub const TERMS_DECLINE: T = ("Çıkış", "Quit");

pub const WHY_TITLE: T = ("Neden gerekli?", "Why do you need it?");
pub const WHY: T = (
    "Bağlandığınız ağ her zaman güvenli değildir. Ortak Wi-Fi'larda, otel ve kafe ağlarında ya da ele geçirilmiş bir modemde ağa sızmış biri trafiğinizi izleyebilir, DNS cevaplarını değiştirip sizi taklit sitelere yönlendirebilir ya da hangi siteleri ziyaret ettiğinizi kaydedebilir. QuickDPI bu tür saldırılara karşı bilgisayarınızda ek bir koruma katmanı çalıştırır. Güvendiğiniz ev ağınızda gerekmez; güvenmediğiniz bir ağa bağlandığınızda açın.",
    "The network you are on is not always safe. On public Wi-Fi, hotel and café networks or behind a compromised router, an intruder can watch your traffic, forge DNS answers to send you to look-alike sites, or log which sites you visit. QuickDPI runs an extra layer of protection against these attacks on your own device. You don't need it on your trusted home network; turn it on when you join a network you don't trust.",
);

pub const DOES_TITLE: T = ("Ne yapar?", "What does it do?");
pub const DOES: &[T] = &[
    (
        "Güvenli DNS: alan adı sorgularınızı seçtiğiniz güvenilir sunucuya, standart dışı bir porttan gönderir. Ağdaki birinin sahte cevapla sizi taklit sitelere yönlendirmesi zorlaşır.",
        "Secure DNS: sends your name lookups to a trusted server of your choice on a non-standard port, making it harder for someone on the network to redirect you to look-alike sites with forged answers.",
    ),
    (
        "Bağlantı gizliliği: bağlantının ziyaret edilen site adını taşıyan ilk paketini parçalara bölerek gönderir. Ağdaki pasif izleme ve paket inceleme cihazlarının bu adı okuması zorlaşır.",
        "Connection privacy: sends the first packet of a connection, which carries the name of the site you visit, in pieces, making it harder for passive monitoring and packet-inspection devices on the network to read it.",
    ),
    (
        "Kendini ayarlar: ilk açılışta ağınıza uyan ayarı kendisi bulur; site kurallarıyla korumanın nerede uygulanacağını siz seçersiniz.",
        "Configures itself: finds the setting that suits your network on first launch; site rules let you choose where protection applies.",
    ),
];

pub const NOT_TITLE: T = ("Ne yapmaz?", "What doesn't it do?");
pub const NOT: T = (
    "QuickDPI bir VPN değildir; trafiğinizi şifrelemez ve IP adresinizi gizlemez. Sitelerin kendi şifrelemesi (HTTPS) olduğu gibi kalır, QuickDPI ona dokunmaz. Hiçbir veri toplamaz; yalnızca seçtiğiniz DNS sunucusuna ve otomatik ayar sırasında test sitelerine bağlanır.",
    "QuickDPI is not a VPN; it does not encrypt your traffic or hide your IP address. Sites' own encryption (HTTPS) stays as it is; QuickDPI never touches it. It collects no data and only connects to the DNS server you choose and, during auto-configuration, to the test sites.",
);

pub const LICENSE: T = (
    "Açık kaynak, MIT lisansı. Windows sürümündeki WinDivert 2.2.2 LGPLv3 lisanslıdır.",
    "Open source, MIT licence. WinDivert 2.2.2 in the Windows build is LGPLv3 licensed.",
);
