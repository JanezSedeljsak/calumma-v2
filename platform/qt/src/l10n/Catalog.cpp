#include "Catalog.hpp"

#include <fstream>
#include <sstream>

namespace calumma::l10n {
namespace {

// A reader for exactly the shape translations/*.json has: one object, string keys, string
// values. Not a JSON library — it refuses anything else rather than half-understanding it, so
// a file that grows nesting or numbers fails loudly at load instead of silently dropping keys.
class FlatJson {
public:
    explicit FlatJson(std::string_view text) : m_text(text) {}

    bool parse(std::unordered_map<std::string, std::string> &out) {
        skipSpace();
        if (!take('{')) {
            return false;
        }
        skipSpace();
        if (take('}')) {
            return true;
        }
        while (true) {
            std::string key;
            std::string value;
            skipSpace();
            if (!readString(key)) {
                return false;
            }
            skipSpace();
            if (!take(':')) {
                return false;
            }
            skipSpace();
            if (!readString(value)) {
                return false;
            }
            out.insert_or_assign(std::move(key), std::move(value));
            skipSpace();
            if (take(',')) {
                continue;
            }
            return take('}');
        }
    }

private:
    void skipSpace() {
        while (m_pos < m_text.size() && (m_text[m_pos] == ' ' || m_text[m_pos] == '\t' ||
                                         m_text[m_pos] == '\n' || m_text[m_pos] == '\r')) {
            ++m_pos;
        }
    }

    bool take(char c) {
        if (m_pos < m_text.size() && m_text[m_pos] == c) {
            ++m_pos;
            return true;
        }
        return false;
    }

    bool readString(std::string &out) {
        if (!take('"')) {
            return false;
        }
        out.clear();
        while (m_pos < m_text.size()) {
            const char c = m_text[m_pos++];
            if (c == '"') {
                return true;
            }
            if (c != '\\') {
                out.push_back(c);
                continue;
            }
            if (m_pos >= m_text.size()) {
                return false;
            }
            const char escaped = m_text[m_pos++];
            switch (escaped) {
            case '"':
            case '\\':
            case '/':
                out.push_back(escaped);
                break;
            case 'n':
                out.push_back('\n');
                break;
            case 't':
                out.push_back('\t');
                break;
            case 'r':
                out.push_back('\r');
                break;
            case 'b':
                out.push_back('\b');
                break;
            case 'f':
                out.push_back('\f');
                break;
            case 'u':
                if (!readUnicodeEscape(out)) {
                    return false;
                }
                break;
            default:
                return false;
            }
        }
        return false;
    }

    // \uXXXX, encoded straight to UTF-8. Surrogate pairs are joined so an emoji or a CJK
    // character written in escaped form survives — the files are UTF-8 already, so this is the
    // uncommon path, but a half surrogate would produce mojibake rather than an error.
    bool readUnicodeEscape(std::string &out) {
        uint32_t code = 0;
        if (!readHex4(code)) {
            return false;
        }
        if (code >= 0xD800 && code <= 0xDBFF) {
            if (m_pos + 1 >= m_text.size() || m_text[m_pos] != '\\' || m_text[m_pos + 1] != 'u') {
                return false;
            }
            m_pos += 2;
            uint32_t low = 0;
            if (!readHex4(low) || low < 0xDC00 || low > 0xDFFF) {
                return false;
            }
            code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
        }
        appendUtf8(code, out);
        return true;
    }

    bool readHex4(uint32_t &out) {
        if (m_pos + 4 > m_text.size()) {
            return false;
        }
        out = 0;
        for (int i = 0; i < 4; ++i) {
            const char c = m_text[m_pos++];
            out <<= 4;
            if (c >= '0' && c <= '9') {
                out |= static_cast<uint32_t>(c - '0');
            } else if (c >= 'a' && c <= 'f') {
                out |= static_cast<uint32_t>(c - 'a' + 10);
            } else if (c >= 'A' && c <= 'F') {
                out |= static_cast<uint32_t>(c - 'A' + 10);
            } else {
                return false;
            }
        }
        return true;
    }

    static void appendUtf8(uint32_t code, std::string &out) {
        if (code < 0x80) {
            out.push_back(static_cast<char>(code));
        } else if (code < 0x800) {
            out.push_back(static_cast<char>(0xC0 | (code >> 6)));
            out.push_back(static_cast<char>(0x80 | (code & 0x3F)));
        } else if (code < 0x10000) {
            out.push_back(static_cast<char>(0xE0 | (code >> 12)));
            out.push_back(static_cast<char>(0x80 | ((code >> 6) & 0x3F)));
            out.push_back(static_cast<char>(0x80 | (code & 0x3F)));
        } else {
            out.push_back(static_cast<char>(0xF0 | (code >> 18)));
            out.push_back(static_cast<char>(0x80 | ((code >> 12) & 0x3F)));
            out.push_back(static_cast<char>(0x80 | ((code >> 6) & 0x3F)));
            out.push_back(static_cast<char>(0x80 | (code & 0x3F)));
        }
    }

    std::string_view m_text;
    size_t m_pos = 0;
};

}  // namespace

bool Catalog::loadJson(std::string_view json) {
    std::unordered_map<std::string, std::string> parsed;
    if (!FlatJson(json).parse(parsed) || parsed.empty()) {
        return false;
    }
    m_strings = std::move(parsed);
    return true;
}

bool Catalog::loadFile(const std::string &path) {
    std::ifstream file(path, std::ios::binary);
    if (!file) {
        return false;
    }
    std::ostringstream buffer;
    buffer << file.rdbuf();
    return loadJson(buffer.str());
}

std::string Catalog::operator()(std::string_view key) const {
    const auto found = m_strings.find(std::string(key));
    return found != m_strings.end() ? found->second : std::string(key);
}

std::string Catalog::format(std::string_view key,
                            std::initializer_list<std::string_view> args) const {
    std::string out = (*this)(key);
    size_t index = 0;
    for (const std::string_view arg : args) {
        const std::string placeholder = "{" + std::to_string(index++) + "}";
        for (size_t at = out.find(placeholder); at != std::string::npos;
             at = out.find(placeholder, at + arg.size())) {
            out.replace(at, placeholder.size(), arg);
        }
    }
    return out;
}

}  // namespace calumma::l10n
