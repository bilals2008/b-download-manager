//! The country of an address, from the table the crate carries.
//!
//! The table is DB-IP's IP to Country Lite database, packed by the reference
//! manager's `tools/geo/build.py` from the monthly CSV and shipped with it
//! (`crates/geo/data/ipcountry.bin.xz`, the 2026-11 edition, carried over
//! unchanged). The addresses are looked up on this computer; none is ever
//! sent anywhere.
//!
//! The data is under the Creative Commons Attribution 4.0 International
//! licence: "IP Geolocation by DB-IP", <https://db-ip.com>. The credit the
//! licence asks for is [`ATTRIBUTION`]; where a country is shown, the credit
//! belongs beside it.
//!
//! The table is unpacked once, on first use (about 3 MB, a few tens of
//! milliseconds). [`prepare`] unpacks it early, on a thread of its own, so
//! the first lookup does not wait for it.

use std::io::Read;
use std::net::IpAddr;
use std::sync::OnceLock;

/// The table, packed.
const TABLE: &[u8] = include_bytes!("ipcountry.bin.xz");

/// What the data's licence asks to be credited.
pub const ATTRIBUTION: &str = "IP Geolocation by DB-IP";

/// Where the data comes from.
pub const ATTRIBUTION_URL: &str = "https://db-ip.com";

/// The licence the data is under.
pub const ATTRIBUTION_LICENCE: &str = "CC BY 4.0";

/// One family's ranges: `count` entries in the unpacked table, and every
/// `SPARSE`-th entry's first address, offset and number, for a binary search.
struct Part {
    count: usize,
    sparse: Vec<(u64, usize, usize)>,
}

/// How many entries one step of the sparse index covers.
const SPARSE: usize = 256;

/// The unpacked table: the countries' codes and the two families of ranges.
struct Table {
    data: Vec<u8>,
    codes: Vec<[u8; 2]>,
    v4: Part,
    v6: Part,
}

/// The table, unpacked once.
static LOADED: OnceLock<Option<Table>> = OnceLock::new();

/// A number written in the LEB128 form the table uses.
fn leb128(data: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = *data.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// A part's entries from `at`: the sparse index, and where the part ends.
fn part(data: &[u8], at: &mut usize) -> Option<Part> {
    let count = u32::from_le_bytes(data.get(*at..*at + 4)?.try_into().ok()?) as usize;
    *at += 4;
    let mut sparse = Vec::with_capacity(count / SPARSE + 1);
    let mut first = 0u64;
    for i in 0..count {
        let entry = *at;
        first = first.checked_add(leb128(data, at)?)?;
        *at += 1;
        if i % SPARSE == 0 {
            sparse.push((first, entry, i));
        }
    }
    Some(Part { count, sparse })
}

/// Unpack the table.
fn load() -> Option<Table> {
    let mut data = Vec::new();
    lzma_rust2::XzReader::new(TABLE, false)
        .read_to_end(&mut data)
        .ok()?;
    if data.get(..7)? != b"MDMGEO\x01" {
        return None;
    }
    let count = *data.get(7)? as usize;
    let codes = (0..count)
        .map(|i| data.get(8 + 2 * i..10 + 2 * i).map(|c| [c[0], c[1]]))
        .collect::<Option<Vec<_>>>()?;
    let mut at = 8 + 2 * count;
    let v4 = part(&data, &mut at)?;
    let v6 = part(&data, &mut at)?;
    Some(Table {
        data,
        codes,
        v4,
        v6,
    })
}

/// The table, unpacked once.
fn table() -> Option<&'static Table> {
    LOADED.get_or_init(load).as_ref()
}

/// Unpack the table now, on a thread of its own, if it is not yet.
pub fn prepare() {
    if LOADED.get().is_none() {
        std::thread::spawn(|| {
            let _ = table();
        });
    }
}

/// The country of `key` in `part`: the last range starting at or before it.
fn find(table: &Table, part: &Part, key: u64) -> u8 {
    if part.count == 0 {
        return 0;
    }
    let at = part.sparse.partition_point(|(first, _, _)| *first <= key);
    let Some(&(mut first, mut at, mut i)) = at.checked_sub(1).and_then(|at| part.sparse.get(at))
    else {
        return 0;
    };
    // The sparse entry's own first address is absolute; walk on from it.
    let mut index = {
        let mut p = at;
        let _ = leb128(&table.data, &mut p);
        let value = table.data.get(p).copied().unwrap_or(0);
        at = p + 1;
        value
    };
    i += 1;
    while i < part.count {
        let mut p = at;
        let Some(delta) = leb128(&table.data, &mut p) else {
            break;
        };
        let next = first.saturating_add(delta);
        if next > key {
            break;
        }
        first = next;
        index = table.data.get(p).copied().unwrap_or(0);
        at = p + 1;
        i += 1;
    }
    index
}

/// A country of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Country {
    /// Its index in the table (1 and up).
    pub index: u8,
    code: [u8; 2],
}

impl Country {
    /// Its ISO 3166-1 alpha-2 code, upper case ("DE").
    pub fn code(&self) -> &str {
        std::str::from_utf8(&self.code).unwrap_or("??")
    }

    /// Its English name ("Germany"), when the crate knows the country.
    pub fn name(&self) -> Option<&'static str> {
        name_of(self.code())
    }
}

/// The country of `ip`: None for an address of this computer or of the local
/// network, a reserved one, or one the table does not place.
///
/// The table is unpacked on first use (see [`prepare`]).
pub fn country(ip: IpAddr) -> Option<Country> {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    if is_local(ip) {
        return None;
    }
    let table = table()?;
    let index = match ip {
        IpAddr::V4(address) => find(table, &table.v4, u64::from(u32::from(address))),
        // The data is never finer than a /64, so the first 64 bits are the key.
        IpAddr::V6(address) => find(table, &table.v6, (u128::from(address) >> 64) as u64),
    };
    let code = *table.codes.get(usize::from(index).checked_sub(1)?)?;
    Some(Country { index, code })
}

/// Whether the address is one of this computer or of the local network: it
/// has no country.
fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(address) => {
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_unspecified()
                || address.is_broadcast()
                || address.is_multicast()
        }
        IpAddr::V6(address) => {
            let first = address.segments()[0];
            address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
    }
}

/// The countries' English names, by their code. The codes are in the order
/// the table keeps them, which is alphabetical.
const NAMES: &[(&str, &str)] = &[
    ("AD", "Andorra"),
    ("AE", "United Arab Emirates"),
    ("AF", "Afghanistan"),
    ("AG", "Antigua and Barbuda"),
    ("AI", "Anguilla"),
    ("AL", "Albania"),
    ("AM", "Armenia"),
    ("AO", "Angola"),
    ("AQ", "Antarctica"),
    ("AR", "Argentina"),
    ("AS", "American Samoa"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("AW", "Aruba"),
    ("AX", "Aland Islands"),
    ("AZ", "Azerbaijan"),
    ("BA", "Bosnia and Herzegovina"),
    ("BB", "Barbados"),
    ("BD", "Bangladesh"),
    ("BE", "Belgium"),
    ("BF", "Burkina Faso"),
    ("BG", "Bulgaria"),
    ("BH", "Bahrain"),
    ("BI", "Burundi"),
    ("BJ", "Benin"),
    ("BL", "Saint Barthélemy"),
    ("BM", "Bermuda"),
    ("BN", "Brunei"),
    ("BO", "Bolivia"),
    ("BQ", "Bonaire, Sint Eustatius and Saba"),
    ("BR", "Brazil"),
    ("BS", "Bahamas"),
    ("BT", "Bhutan"),
    ("BV", "Bouvet Island"),
    ("BW", "Botswana"),
    ("BY", "Belarus"),
    ("BZ", "Belize"),
    ("CA", "Canada"),
    ("CC", "Cocos (Keeling) Islands"),
    ("CD", "DR Congo"),
    ("CF", "Central African Republic"),
    ("CG", "Congo"),
    ("CH", "Switzerland"),
    ("CI", "Côte d'Ivoire"),
    ("CK", "Cook Islands"),
    ("CL", "Chile"),
    ("CM", "Cameroon"),
    ("CN", "China"),
    ("CO", "Colombia"),
    ("CR", "Costa Rica"),
    ("CU", "Cuba"),
    ("CV", "Cabo Verde"),
    ("CW", "Curaçao"),
    ("CX", "Christmas Island"),
    ("CY", "Cyprus"),
    ("CZ", "Czech Republic"),
    ("DE", "Germany"),
    ("DJ", "Djibouti"),
    ("DK", "Denmark"),
    ("DM", "Dominica"),
    ("DO", "Dominican Republic"),
    ("DZ", "Algeria"),
    ("EC", "Ecuador"),
    ("EE", "Estonia"),
    ("EG", "Egypt"),
    ("EH", "Western Sahara"),
    ("ER", "Eritrea"),
    ("ES", "Spain"),
    ("ET", "Ethiopia"),
    ("FI", "Finland"),
    ("FJ", "Fiji"),
    ("FK", "Falkland Islands"),
    ("FM", "Micronesia"),
    ("FO", "Faroe Islands"),
    ("FR", "France"),
    ("GA", "Gabon"),
    ("GB", "United Kingdom"),
    ("GD", "Grenada"),
    ("GE", "Georgia"),
    ("GF", "French Guiana"),
    ("GG", "Guernsey"),
    ("GH", "Ghana"),
    ("GI", "Gibraltar"),
    ("GL", "Greenland"),
    ("GM", "Gambia"),
    ("GN", "Guinea"),
    ("GP", "Guadeloupe"),
    ("GQ", "Equatorial Guinea"),
    ("GR", "Greece"),
    ("GS", "South Georgia and the South Sandwich Islands"),
    ("GT", "Guatemala"),
    ("GU", "Guam"),
    ("GW", "Guinea-Bissau"),
    ("GY", "Guyana"),
    ("HK", "Hong Kong"),
    ("HM", "Heard Island and McDonald Islands"),
    ("HN", "Honduras"),
    ("HR", "Croatia"),
    ("HT", "Haiti"),
    ("HU", "Hungary"),
    ("ID", "Indonesia"),
    ("IE", "Ireland"),
    ("IL", "Israel"),
    ("IM", "Isle of Man"),
    ("IN", "India"),
    ("IO", "British Indian Ocean Territory"),
    ("IQ", "Iraq"),
    ("IR", "Iran"),
    ("IS", "Iceland"),
    ("IT", "Italy"),
    ("JE", "Jersey"),
    ("JM", "Jamaica"),
    ("JO", "Jordan"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("KG", "Kyrgyzstan"),
    ("KH", "Cambodia"),
    ("KI", "Kiribati"),
    ("KM", "Comoros"),
    ("KN", "Saint Kitts and Nevis"),
    ("KP", "North Korea"),
    ("KR", "South Korea"),
    ("KW", "Kuwait"),
    ("KY", "Cayman Islands"),
    ("KZ", "Kazakhstan"),
    ("LA", "Laos"),
    ("LB", "Lebanon"),
    ("LC", "Saint Lucia"),
    ("LI", "Liechtenstein"),
    ("LK", "Sri Lanka"),
    ("LR", "Liberia"),
    ("LS", "Lesotho"),
    ("LT", "Lithuania"),
    ("LU", "Luxembourg"),
    ("LV", "Latvia"),
    ("LY", "Libya"),
    ("MA", "Morocco"),
    ("MC", "Monaco"),
    ("MD", "Moldova"),
    ("ME", "Montenegro"),
    ("MF", "Saint Martin"),
    ("MG", "Madagascar"),
    ("MH", "Marshall Islands"),
    ("MK", "North Macedonia"),
    ("ML", "Mali"),
    ("MM", "Myanmar"),
    ("MN", "Mongolia"),
    ("MO", "Macau"),
    ("MP", "Northern Mariana Islands"),
    ("MQ", "Martinique"),
    ("MR", "Mauritania"),
    ("MS", "Montserrat"),
    ("MT", "Malta"),
    ("MU", "Mauritius"),
    ("MV", "Maldives"),
    ("MW", "Malawi"),
    ("MX", "Mexico"),
    ("MY", "Malaysia"),
    ("MZ", "Mozambique"),
    ("NA", "Namibia"),
    ("NC", "New Caledonia"),
    ("NE", "Niger"),
    ("NF", "Norfolk Island"),
    ("NG", "Nigeria"),
    ("NI", "Nicaragua"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NP", "Nepal"),
    ("NR", "Nauru"),
    ("NU", "Niue"),
    ("NZ", "New Zealand"),
    ("OM", "Oman"),
    ("PA", "Panama"),
    ("PE", "Peru"),
    ("PF", "French Polynesia"),
    ("PG", "Papua New Guinea"),
    ("PH", "Philippines"),
    ("PK", "Pakistan"),
    ("PL", "Poland"),
    ("PM", "Saint Pierre and Miquelon"),
    ("PN", "Pitcairn"),
    ("PR", "Puerto Rico"),
    ("PS", "Palestine"),
    ("PT", "Portugal"),
    ("PW", "Palau"),
    ("PY", "Paraguay"),
    ("QA", "Qatar"),
    ("RE", "Réunion"),
    ("RO", "Romania"),
    ("RS", "Serbia"),
    ("RU", "Russia"),
    ("RW", "Rwanda"),
    ("SA", "Saudi Arabia"),
    ("SB", "Solomon Islands"),
    ("SC", "Seychelles"),
    ("SD", "Sudan"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("SH", "Saint Helena, Ascension and Tristan da Cunha"),
    ("SI", "Slovenia"),
    ("SJ", "Svalbard and Jan Mayen"),
    ("SK", "Slovakia"),
    ("SL", "Sierra Leone"),
    ("SM", "San Marino"),
    ("SN", "Senegal"),
    ("SO", "Somalia"),
    ("SR", "Suriname"),
    ("SS", "South Sudan"),
    ("ST", "Sao Tome and Principe"),
    ("SV", "El Salvador"),
    ("SX", "Sint Maarten"),
    ("SY", "Syria"),
    ("SZ", "Eswatini"),
    ("TC", "Turks and Caicos Islands"),
    ("TD", "Chad"),
    ("TF", "French Southern Territories"),
    ("TG", "Togo"),
    ("TH", "Thailand"),
    ("TJ", "Tajikistan"),
    ("TK", "Tokelau"),
    ("TL", "Timor-Leste"),
    ("TM", "Turkmenistan"),
    ("TN", "Tunisia"),
    ("TO", "Tonga"),
    ("TR", "Türkiye"),
    ("TT", "Trinidad and Tobago"),
    ("TV", "Tuvalu"),
    ("TW", "Taiwan"),
    ("TZ", "Tanzania"),
    ("UA", "Ukraine"),
    ("UG", "Uganda"),
    ("UM", "United States Minor Outlying Islands"),
    ("US", "United States"),
    ("UY", "Uruguay"),
    ("UZ", "Uzbekistan"),
    ("VA", "Vatican City"),
    ("VC", "Saint Vincent and the Grenadines"),
    ("VE", "Venezuela"),
    ("VG", "Virgin Islands (British)"),
    ("VI", "Virgin Islands (U.S.)"),
    ("VN", "Vietnam"),
    ("VU", "Vanuatu"),
    ("WF", "Wallis and Futuna"),
    ("WS", "Samoa"),
    ("XK", "Kosovo"),
    ("YE", "Yemen"),
    ("YT", "Mayotte"),
    ("ZA", "South Africa"),
    ("ZM", "Zambia"),
    ("ZW", "Zimbabwe"),
];

/// The name of the country with the code `code` (upper case), or None.
fn name_of(code: &str) -> Option<&'static str> {
    NAMES
        .binary_search_by(|(known, _)| (*known).cmp(code))
        .ok()
        .map(|at| NAMES[at].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(ip: &str) -> Option<String> {
        country(ip.parse().expect("the address")).map(|c| c.code().to_owned())
    }

    #[test]
    fn addresses_have_their_countries() {
        // Well-known, long-held blocks: DB-IP's sample row (8.8.8.0/24, US),
        // APNIC's 1.0.0.0/24 (AU) and 1.0.1.0/24 (CN).
        assert_eq!(code("8.8.8.8").as_deref(), Some("US"));
        assert_eq!(code("1.0.0.1").as_deref(), Some("AU"));
        assert_eq!(code("1.0.2.3").as_deref(), Some("CN"));
        // The same address, IPv4-mapped.
        assert_eq!(code("::ffff:8.8.8.8").as_deref(), Some("US"));
        // An IPv6 address is placed (which country moves with the monthly
        // data).
        assert!(code("2001:4860:4860::8888").is_some());
    }

    #[test]
    fn this_computer_and_the_local_network_have_none() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.1",
            "169.254.1.1",
            "::1",
            "fe80::1",
            "fd00::1",
            "0.0.0.0",
        ] {
            assert_eq!(code(ip), None, "{ip}");
        }
    }

    #[test]
    fn a_country_knows_its_name() {
        let country = country("8.8.8.8".parse().expect("the address")).expect("a country");
        assert_eq!(country.code(), "US");
        assert_eq!(country.name(), Some("United States"));
        assert_eq!(name_of("DE"), Some("Germany"));
        assert_eq!(name_of("ZZ"), None);
    }

    #[test]
    fn the_names_are_in_the_order_the_table_keeps_them() {
        // The table's countries are the codes of the names, in the same order.
        let table = table().expect("the table is unpacked");
        assert_eq!(table.codes.len(), NAMES.len());
        for (at, (code, _)) in NAMES.iter().enumerate() {
            assert_eq!(
                std::str::from_utf8(&table.codes[at]).unwrap_or("??"),
                *code,
                "country {at}"
            );
        }
    }
}
