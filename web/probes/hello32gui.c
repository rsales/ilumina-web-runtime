/* Layer-1 probe for ADR 0007: the smallest 32-bit GUI Windows program.
 * Shows a message box and writes C:\boxedwine-probe.txt, so it can be checked
 * visually in the browser and from the persisted file system.
 * Build: i686-w64-mingw32-gcc -O1 -nostdlib -e _start@0 -mwindows -o hello32gui.exe hello32gui.c -lkernel32 -luser32 */
#include <windows.h>
void __stdcall start(void) {
    DWORD n; const char m[] = "WOW64-32BIT-OK (Boxedwine probe)\r\n";
    HANDLE f = CreateFileA("C:\\boxedwine-probe.txt", GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, 0, NULL);
    if (f != INVALID_HANDLE_VALUE) { WriteFile(f, m, sizeof m - 1, &n, NULL); CloseHandle(f); }
    MessageBoxA(NULL, "Programa Windows x86 (PE32) rodando.\nCamada 1 (emulador) OK.", "iLumina - probe 32-bit", MB_OK);
    ExitProcess(0);
}
