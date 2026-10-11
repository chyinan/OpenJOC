// SPDX-License-Identifier: Apache-2.0
// Harmless Windows GUI-subsystem process fixture. It never opens the fake
// module argument, loads a DLL, starts another program, or accesses the registry.
// Compile in an existing MSVC developer shell (no downloads required).
// Use /EHsc /std:c++17 /DUNICODE /D_UNICODE, and link with
// /SUBSYSTEM:WINDOWS Shell32.lib Kernel32.lib.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <string>
#include <vector>
#include <cwchar>

namespace {
bool ReadEnvironment(const wchar_t* name, std::wstring& value) {
    const DWORD required = GetEnvironmentVariableW(name, nullptr, 0);
    if (required == 0) return false;
    std::vector<wchar_t> buffer(required);
    const DWORD copied = GetEnvironmentVariableW(name, buffer.data(), required);
    if (copied == 0 || copied >= required) return false;
    value.assign(buffer.data(), copied);
    return true;
}

bool FullPath(const std::wstring& input, std::wstring& output) {
    const DWORD required = GetFullPathNameW(input.c_str(), 0, nullptr, nullptr);
    if (required == 0) return false;
    std::vector<wchar_t> buffer(required);
    const DWORD copied = GetFullPathNameW(input.c_str(), required, buffer.data(), nullptr);
    if (copied == 0 || copied >= required) return false;
    output.assign(buffer.data(), copied);
    return true;
}

bool IsInsideRunnerTemp(const std::wstring& marker, const std::wstring& runnerTemp) {
    std::wstring fullMarker, fullRoot;
    if (!FullPath(marker, fullMarker) || !FullPath(runnerTemp, fullRoot)) return false;
    if (fullRoot.empty()) return false;
    if (fullRoot.back() != L'\\') fullRoot += L'\\';
    return fullMarker.size() > fullRoot.size() &&
        _wcsnicmp(fullMarker.c_str(), fullRoot.c_str(), fullRoot.size()) == 0;
}
}  // namespace

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    std::wstring module, operation, marker, exitText, runnerTemp;
    if (!ReadEnvironment(L"LAV_PROBE_EXPECTED_MODULE", module) ||
        !ReadEnvironment(L"LAV_PROBE_EXPECTED_OPERATION", operation) ||
        !ReadEnvironment(L"LAV_PROBE_MARKER_PATH", marker) ||
        !ReadEnvironment(L"LAV_PROBE_EXIT_CODE", exitText) ||
        !ReadEnvironment(L"RUNNER_TEMP", runnerTemp)) return 91;
    if (exitText != L"0" && exitText != L"7") return 91;
    if (operation != L"Register" && operation != L"Unregister") return 91;
    if (!IsInsideRunnerTemp(marker, runnerTemp)) return 94;

    int argc = 0;
    LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    if (argv == nullptr) return 92;
    const bool unregister = operation == L"Unregister";
    bool matches = argc == (unregister ? 4 : 3);
    if (matches && unregister) {
        matches = std::wcscmp(argv[1], L"/u") == 0 &&
                  std::wcscmp(argv[2], L"/s") == 0 && module == argv[3];
    } else if (matches) {
        matches = std::wcscmp(argv[1], L"/s") == 0 && module == argv[2];
    }
    LocalFree(argv);
    if (!matches) return 92;

    // The parent must not return successfully until this delayed marker exists.
    Sleep(700);
    const std::string receipt = std::string("{\"completed\":true,\"argv_match\":true,\"argc\":") +
        (unregister ? "4" : "3") + ",\"exit_code\":" +
        (exitText == L"7" ? "7" : "0") + "}\r\n";
    HANDLE file = CreateFileW(marker.c_str(), GENERIC_WRITE, 0, nullptr,
                              CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return 93;
    DWORD written = 0;
    const DWORD size = static_cast<DWORD>(receipt.size());
    const bool saved = WriteFile(file, receipt.data(), size, &written, nullptr) != FALSE &&
                       written == size && FlushFileBuffers(file) != FALSE;
    CloseHandle(file);
    if (!saved) return 93;
    return exitText == L"7" ? 7 : 0;
}
