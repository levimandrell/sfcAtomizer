// snes_spc_oracle — thin SPC render wrapper for the SFC Wave Compiler
// M0.5 calibration boundary.
//
// LGPL-2.1+ (links the LGPL snes_spc sources statically). See
// vendor/snes_spc/license.txt and the project root LICENSING.md for
// the LGPL compliance statement; this binary is only invoked across
// a process boundary by the Apache-2.0 host.
//
// CLI contract v1 — locked, do not extend without PM brief:
//
//   snes_spc_oracle --version
//   snes_spc_oracle render --input-spc <path> --frames <N>
//                          --output-pcm <path> --report <path>
//
// Output: signed 16-bit interleaved stereo PCM at 32 kHz, plus a
// JSON report. Determinism: identical input + identical wrapper
// build → identical PCM bytes.

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

extern "C" {
#include "snes_spc/spc.h"
}

#define WRAPPER_VERSION "0.1.0"
#define SNES_SPC_PIN "ec8ee2bbe30451614c1d02a83f7af1c97d497d45"
#define SAMPLE_RATE_HZ 32000

namespace fs = std::filesystem;

// ============================================================================
// SHA-256 (FIPS 180-4) — single-file impl, public-domain style.
// Adapted from Brad Conte's reference (B-Con/crypto-algorithms).
// ============================================================================

namespace {

constexpr uint32_t K256[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
    0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
    0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

inline uint32_t rotr(uint32_t x, int n) { return (x >> n) | (x << (32 - n)); }

struct Sha256 {
    uint8_t buf[64] = {0};
    uint32_t buflen = 0;
    uint64_t bitlen = 0;
    uint32_t H[8] = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                     0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19};

    void transform(const uint8_t* data) {
        uint32_t m[64], a, b, c, d, e, f, g, h, t1, t2;
        for (int i = 0, j = 0; i < 16; ++i, j += 4) {
            m[i] = (uint32_t(data[j]) << 24) | (uint32_t(data[j + 1]) << 16) |
                   (uint32_t(data[j + 2]) << 8) | uint32_t(data[j + 3]);
        }
        for (int i = 16; i < 64; ++i) {
            uint32_t s0 = rotr(m[i - 15], 7) ^ rotr(m[i - 15], 18) ^ (m[i - 15] >> 3);
            uint32_t s1 = rotr(m[i - 2], 17) ^ rotr(m[i - 2], 19) ^ (m[i - 2] >> 10);
            m[i] = m[i - 16] + s0 + m[i - 7] + s1;
        }
        a = H[0]; b = H[1]; c = H[2]; d = H[3];
        e = H[4]; f = H[5]; g = H[6]; h = H[7];
        for (int i = 0; i < 64; ++i) {
            uint32_t S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
            uint32_t ch = (e & f) ^ (~e & g);
            t1 = h + S1 + ch + K256[i] + m[i];
            uint32_t S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
            uint32_t mj = (a & b) ^ (a & c) ^ (b & c);
            t2 = S0 + mj;
            h = g; g = f; f = e; e = d + t1;
            d = c; c = b; b = a; a = t1 + t2;
        }
        H[0] += a; H[1] += b; H[2] += c; H[3] += d;
        H[4] += e; H[5] += f; H[6] += g; H[7] += h;
    }

    void update(const uint8_t* data, size_t len) {
        for (size_t i = 0; i < len; ++i) {
            buf[buflen++] = data[i];
            if (buflen == 64) {
                transform(buf);
                bitlen += 512;
                buflen = 0;
            }
        }
    }

    void final_digest(uint8_t out[32]) {
        bitlen += uint64_t(buflen) * 8;
        buf[buflen++] = 0x80;
        if (buflen > 56) {
            while (buflen < 64) buf[buflen++] = 0;
            transform(buf);
            buflen = 0;
        }
        while (buflen < 56) buf[buflen++] = 0;
        for (int i = 0; i < 8; ++i) {
            buf[63 - i] = uint8_t(bitlen >> (i * 8));
        }
        transform(buf);
        for (int i = 0; i < 8; ++i) {
            for (int j = 0; j < 4; ++j) {
                out[i * 4 + j] = uint8_t(H[i] >> (24 - j * 8));
            }
        }
    }
};

std::string sha256_hex(const uint8_t* data, size_t len) {
    Sha256 ctx;
    ctx.update(data, len);
    uint8_t digest[32];
    ctx.final_digest(digest);
    char hex[65];
    for (int i = 0; i < 32; ++i) {
        std::snprintf(hex + i * 2, 3, "%02x", digest[i]);
    }
    hex[64] = '\0';
    return std::string(hex);
}

// ============================================================================
// I/O + arg helpers
// ============================================================================

std::vector<uint8_t> read_file(const char* path) {
    std::ifstream in(path, std::ios::binary);
    if (!in) throw std::runtime_error(std::string("cannot open: ") + path);
    in.seekg(0, std::ios::end);
    std::streamsize len = in.tellg();
    if (len < 0) throw std::runtime_error(std::string("cannot size: ") + path);
    in.seekg(0, std::ios::beg);
    std::vector<uint8_t> bytes(static_cast<size_t>(len));
    if (len > 0 && !in.read(reinterpret_cast<char*>(bytes.data()), len)) {
        throw std::runtime_error(std::string("cannot read: ") + path);
    }
    return bytes;
}

void write_file(const char* path, const uint8_t* data, size_t len) {
    fs::path p(path);
    if (p.has_parent_path()) {
        std::error_code ec;
        fs::create_directories(p.parent_path(), ec);
    }
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out) throw std::runtime_error(std::string("cannot create: ") + path);
    if (len > 0) out.write(reinterpret_cast<const char*>(data), static_cast<std::streamsize>(len));
    if (!out) throw std::runtime_error(std::string("cannot write: ") + path);
}

const char* arg_value(int argc, char** argv, const char* name) {
    for (int i = 1; i + 1 < argc; ++i) {
        if (std::strcmp(argv[i], name) == 0) return argv[i + 1];
    }
    return nullptr;
}

std::string json_escape(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 8);
    for (char c : s) {
        switch (c) {
            case '"':  out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\n': out += "\\n";  break;
            case '\r': out += "\\r";  break;
            case '\t': out += "\\t";  break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out += c;
                }
        }
    }
    return out;
}

std::string abs_path(const char* path) {
    std::error_code ec;
    fs::path p = fs::absolute(path, ec);
    return ec ? std::string(path) : p.generic_string();
}

void write_error_report(const char* report_path, const std::string& msg,
                        const char* input_spc, const char* output_pcm) {
    std::ostringstream js;
    js << "{\n";
    js << "  \"schema_version\": 1,\n";
    js << "  \"report_type\": \"snes_spc_oracle_render\",\n";
    js << "  \"status\": \"error\",\n";
    js << "  \"error\": \"" << json_escape(msg) << "\",\n";
    js << "  \"wrapper_version\": \"" << WRAPPER_VERSION << "\",\n";
    js << "  \"snes_spc_pin\": \"" << SNES_SPC_PIN << "\",\n";
    js << "  \"input_spc_path\": \"" << (input_spc ? json_escape(abs_path(input_spc)) : std::string()) << "\",\n";
    js << "  \"output_pcm_path\": \"" << (output_pcm ? json_escape(abs_path(output_pcm)) : std::string()) << "\"\n";
    js << "}\n";
    std::string s = js.str();
    try {
        write_file(report_path, reinterpret_cast<const uint8_t*>(s.data()), s.size());
    } catch (...) {
        // best-effort: dump to stderr if even the report can't be written
        std::fprintf(stderr, "snes_spc_oracle: cannot write error report: %s\n", report_path);
    }
}

// ============================================================================
// render
// ============================================================================

int cmd_render(int argc, char** argv) {
    const char* input_spc = arg_value(argc, argv, "--input-spc");
    const char* frames_str = arg_value(argc, argv, "--frames");
    const char* output_pcm = arg_value(argc, argv, "--output-pcm");
    const char* report_path = arg_value(argc, argv, "--report");

    if (!input_spc || !frames_str || !output_pcm || !report_path) {
        std::fprintf(stderr,
                     "render: missing required arg (need --input-spc, --frames, --output-pcm, --report)\n");
        return 2;
    }
    long frames = std::strtol(frames_str, nullptr, 10);
    if (frames <= 0 || frames > (1L << 24)) {
        std::fprintf(stderr, "render: --frames out of range (got %s)\n", frames_str);
        return 2;
    }

    std::vector<uint8_t> spc_bytes;
    try {
        spc_bytes = read_file(input_spc);
    } catch (const std::exception& e) {
        write_error_report(report_path, std::string("read input SPC: ") + e.what(),
                           input_spc, output_pcm);
        return 1;
    }
    std::string spc_sha = sha256_hex(spc_bytes.data(), spc_bytes.size());

    SNES_SPC* spc = spc_new();
    if (!spc) {
        write_error_report(report_path, "spc_new returned null (OOM?)", input_spc, output_pcm);
        return 1;
    }
    spc_err_t err = spc_load_spc(spc, spc_bytes.data(), static_cast<long>(spc_bytes.size()));
    if (err) {
        write_error_report(report_path, std::string("spc_load_spc: ") + err,
                           input_spc, output_pcm);
        spc_delete(spc);
        return 1;
    }
    spc_clear_echo(spc);

    const int sample_count = static_cast<int>(frames * 2);
    std::vector<int16_t> pcm(static_cast<size_t>(sample_count));
    err = spc_play(spc, sample_count, pcm.data());
    spc_delete(spc);
    if (err) {
        write_error_report(report_path, std::string("spc_play: ") + err,
                           input_spc, output_pcm);
        return 1;
    }

    const size_t pcm_bytes = pcm.size() * sizeof(int16_t);
    try {
        write_file(output_pcm, reinterpret_cast<const uint8_t*>(pcm.data()), pcm_bytes);
    } catch (const std::exception& e) {
        write_error_report(report_path, std::string("write PCM: ") + e.what(),
                           input_spc, output_pcm);
        return 1;
    }
    std::string pcm_sha = sha256_hex(reinterpret_cast<const uint8_t*>(pcm.data()), pcm_bytes);

    int32_t max_abs = 0;
    long double sum_sq = 0.0L;
    for (int16_t s : pcm) {
        int32_t a = std::abs(static_cast<int32_t>(s));
        if (a > max_abs) max_abs = a;
        sum_sq += static_cast<long double>(s) * static_cast<long double>(s);
    }
    double rms = pcm.empty() ? 0.0 : std::sqrt(static_cast<double>(sum_sq / static_cast<long double>(pcm.size())));

    std::ostringstream js;
    js << "{\n";
    js << "  \"schema_version\": 1,\n";
    js << "  \"report_type\": \"snes_spc_oracle_render\",\n";
    js << "  \"status\": \"ok\",\n";
    js << "  \"wrapper_version\": \"" << WRAPPER_VERSION << "\",\n";
    js << "  \"snes_spc_pin\": \"" << SNES_SPC_PIN << "\",\n";
    js << "  \"input_spc_path\": \"" << json_escape(abs_path(input_spc)) << "\",\n";
    js << "  \"input_spc_sha256\": \"" << spc_sha << "\",\n";
    js << "  \"frames_rendered\": " << frames << ",\n";
    js << "  \"sample_rate_hz\": " << SAMPLE_RATE_HZ << ",\n";
    js << "  \"channels\": 2,\n";
    js << "  \"bytes_per_sample\": 2,\n";
    js << "  \"output_pcm_path\": \"" << json_escape(abs_path(output_pcm)) << "\",\n";
    js << "  \"output_pcm_sha256\": \"" << pcm_sha << "\",\n";
    js << "  \"output_pcm_max_abs\": " << max_abs << ",\n";
    js << "  \"output_pcm_rms\": " << std::fixed << std::setprecision(6) << rms << "\n";
    js << "}\n";

    try {
        std::string s = js.str();
        write_file(report_path, reinterpret_cast<const uint8_t*>(s.data()), s.size());
    } catch (const std::exception& e) {
        std::fprintf(stderr, "snes_spc_oracle: cannot write report: %s\n", e.what());
        return 1;
    }
    return 0;
}

}  // namespace

int main(int argc, char** argv) {
    if (argc >= 2 && std::strcmp(argv[1], "--version") == 0) {
        std::printf("snes_spc_oracle %s (snes_spc %s)\n", WRAPPER_VERSION, SNES_SPC_PIN);
        return 0;
    }
    if (argc >= 2 && std::strcmp(argv[1], "render") == 0) {
        return cmd_render(argc, argv);
    }
    std::fprintf(stderr,
                 "snes_spc_oracle %s (snes_spc %s)\n"
                 "usage:\n"
                 "  snes_spc_oracle --version\n"
                 "  snes_spc_oracle render --input-spc <path> --frames <N> "
                 "--output-pcm <path> --report <path>\n",
                 WRAPPER_VERSION, SNES_SPC_PIN);
    return 2;
}
