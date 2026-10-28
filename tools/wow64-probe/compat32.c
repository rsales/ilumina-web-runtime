/*
 * compat32 — does this x86_64 environment run 32-bit "compatibility mode" code?
 *
 * Wine's new WoW64 runs 32-bit Windows programs by far-jumping from 64-bit
 * code into a 32-bit code segment (CS=0x23 on Linux) inside ONE 64-bit
 * process. This probe does exactly that switch and nothing else, without
 * Wine. If it crashes, no WoW64 build of Wine can work in this environment.
 *
 * Build: gcc -O0 -static -no-pie -o compat32 compat32.c   (needs no-pie: the
 *        32-bit code must sit below 4 GiB)
 * Exit:  0 + "COMPAT32-OK" = supported; SIGSEGV (139) = not supported.
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <sys/mman.h>

extern long go32(void *low_stack_top);

__asm__(
    ".text\n"
    ".globl go32\n"
    "go32:\n"
    "  push %rbx\n"
    "  push %rbp\n"
    "  mov  %rsp, %rbp\n"          /* save 64-bit stack                   */
    "  mov  %rdi, %rsp\n"          /* stack below 4 GiB for 32-bit mode   */
    "  lea  code32(%rip), %rax\n"
    "  pushq $0x23\n"              /* __USER32_CS                          */
    "  push %rax\n"
    "  lretq\n"                    /* -> 32-bit code                       */
    ".code32\n"
    "code32:\n"
    "  movl $42, %eax\n"
    "  pushl $0x33\n"              /* __USER_CS (64-bit)                   */
    "  pushl $back64\n"
    "  lret\n"                     /* -> back to 64-bit                    */
    ".code64\n"
    "back64:\n"
    "  mov  %rbp, %rsp\n"
    "  pop  %rbp\n"
    "  pop  %rbx\n"
    "  ret\n");

int main(void) {
    char *stack = mmap(NULL, 65536, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS | MAP_32BIT, -1, 0);
    if (stack == MAP_FAILED) { perror("mmap"); return 2; }
    printf("switching to 32-bit compatibility mode...\n"); fflush(stdout);
    long r = go32(stack + 65536 - 16);
    if (r == 42) { printf("COMPAT32-OK\n"); return 0; }
    printf("COMPAT32-WRONG-RESULT %ld\n", r); return 1;
}
