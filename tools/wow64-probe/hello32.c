/* Minimal PE32 (i386) console program for Wine WoW64 tests: no CRT, only kernel32. */
#include <windows.h>
void __stdcall start(void) {
    DWORD n; const char m[] = "WOW64-32BIT-OK\r\n";
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), m, sizeof m - 1, &n, NULL);
    ExitProcess(0);
}
