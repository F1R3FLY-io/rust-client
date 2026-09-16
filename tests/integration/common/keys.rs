//! Keys the suite deploys with.
//!
//! Every deploying test owns one, funded in `topology/genesis/wallets.txt`, so
//! no test funds another and concurrent deploys never share a deployer.

pub struct Key {
    pub private: &'static str,
    pub public: &'static str,
    pub address: &'static str,
}

pub const DEPLOY_A: Key = Key {
    private: "d00874be7317879581b01e70a1d67d6483c04958cdd485669559c901362ae277",
    public: "0443cef93c38aa007d406307b779af0e3015fed5092ddef83b49edea58cef1728f00eb8ac082ec09e41263db19ec0319586ebe4eaba30d98a703e6e900ae658da7",
    address: "1111zBFKwvsLJhrUEup8KR3RyQCDK9ZYwHKmZM9ZNFXYp6uJAPqHa",
};

pub const DEPLOY_B: Key = Key {
    private: "4d518d6372f82c6945132827c4f92b0704219ab2d704461d20b9d6e564abba09",
    public: "04153ddb9c2fdd55a95a9d13564487497df47ddfd2c84fbddae883891f4b7b56309ae96c8a7973e15977c22046a6f1e6c546b674d8edb8f26749e72bf624c3fbe6",
    address: "1111C1shHpaqvPMCrsKrNdRATmFjiCUVzWiXcL4FNFR4vPCz1vDQC",
};

pub const TRANSFER_SENDER: Key = Key {
    private: "a37f06d6bc62db08fd944ced008488fbfc7dbc9bcc491a72f2d1154594526558",
    public: "04330d0ce4517f0d7a2b23d0973e67197bb160ae1f8e5caa50ae62ead0a6357c48cdb721ccd30da10e82abd6c7f3be974d32d1815ec8bad04988d0c07a2f5ba936",
    address: "11112sXU9SzpUXzovWMKrqK2coYdyKoE3JAPxxLn1NtvrYHvvVRqWu",
};

pub const TRANSFER_RECIPIENT: Key = Key {
    private: "6606dc26f06483655714db17d24f3fb688a3dc5011643f427cae6ebd1de33758",
    public: "045b10b4c724937b9e909797507827d9178b77a87c654a79a49a35d1eb7150c8c268cb6131d802f771d3219732453193af13435c8d3966d010ed66a19326cf94f0",
    address: "11112pssxSsf1zfUHmtmAg1EYgCmof2qWcH9Qrmi4p4usH1yVpT9A4",
};

/// Funded with far less than the others, so a transfer of an ordinary amount
/// overdraws the vault.
pub const OVERDRAFT: Key = Key {
    private: "e23fe75dc0700c2c3d4d888d3c00f961992dc262ef972a3f15e6f7dfc864c002",
    public: "04c214ce1c243c31917d834ef8462a63e367bc9764f434a3279b86a1a9a944e29686c850c36aa38fe68d8de395c482df32afcdca7e7be8c34337bb59f181669cab",
    address: "1111tVVZEKhPoAQfE8NRvypFa7c7FGmBvzNcBrVHPByGx5gAsMd42",
};

pub const LOAD_TEST: Key = Key {
    private: "6fbb857a48350ce2cfde38ddc134e8cb17cf45f8b8791e2ec2fb0cbc184700e4",
    public: "04cd6066f89dd7ff3d2fab67e9019523cb99c36ffe967bf7e91538ceb3cbf988a5ad9bbc8d8219b40f171e01fcf3a38aa57502d522f77cfef8852faf6707664fba",
    address: "1111gJNNb8TCwQ7jRJfVwCWDfYHNLzwgL9tJyrwvZeS9zm35UZjht",
};

pub const SPARE: Key = Key {
    private: "117a475bc767512deabc68c29351276997a0e53568724445d7458bebcee84557",
    public: "04d772eaa738217515905b36fe59c40090cea3e4ec385e7c23db8b8aa5fd901120183031681549916805764e7ecfb1ed4a0a88b96831275690a4a496903823a45c",
    address: "1111qmE6aMxLQ1ou9QXnwDnevNauGXP6GCGrZpy5b4zHoQRaRjsku",
};

/// The joiner's validator key, matching `--validator-*-key` on validator4 in
/// the topology. Funded at genesis so bonding needs no transfer first.
pub const VALIDATOR4: Key = Key {
    private: "5ff3514bf79a7d18e8dd974c699678ba63b7762ce8d78c532346e52f0ad219cd",
    public: "04d26c6103d7269773b943d7a9c456f9eb227e0d8b1fe30bccee4fca963f4446e3385d99f6386317f2c1ad36b9e6b0d5f97bb0a0041f05781c60a5ebca124a251d",
    address: "1111La6tHaCtGjRiv4wkffbTAAjGyMsVhzSUNzQxH1jjZH9jtEi3M",
};

/// Bonded at genesis; the topology runs it as validator1.
pub const VALIDATOR1_PUBLIC: &str = "04fa70d7be5eb750e0915c0f6d19e7085d18bb1c22d030feb2a877ca2cd226d04438aa819359c56c720142fbc66e9da03a5ab960a3d8b75363a226b7c800f60420";

/// The secp256k1 generator point: a well-formed public key that no shard can
/// have bonded, for the negative side of the bond-status assertions.
pub const NEVER_BONDED_PUBLIC: &str = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";
