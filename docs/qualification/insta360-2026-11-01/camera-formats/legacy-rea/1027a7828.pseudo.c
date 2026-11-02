
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Removing unreachable block (ram,0x0001027a8768) */
/* WARNING: Removing unreachable block (ram,0x0001027a8774) */
/* WARNING: Removing unreachable block (ram,0x0001027a8784) */
/* WARNING: Removing unreachable block (ram,0x0001027a879c) */
/* WARNING: Removing unreachable block (ram,0x0001027a8750) */
/* WARNING: Removing unreachable block (ram,0x0001027a875c) */
/* WARNING: Removing unreachable block (ram,0x0001027a87a4) */
/* WARNING: Removing unreachable block (ram,0x0001027a87b4) */
/* WARNING: Removing unreachable block (ram,0x0001027a87c4) */
/* WARNING: Removing unreachable block (ram,0x0001027a87cc) */
/* WARNING: Removing unreachable block (ram,0x0001027a8820) */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(undefined8 *param_1,uint param_2)

{
  uint uVar1;
  undefined8 uVar2;
  long lVar3;
  
  uVar2 = _DAT_1037c4eb0;
  lVar3 = *_DAT_103fed798;
  uVar1 = (param_2 & 0xff) - 1;
  if (uVar1 < 0xf2) {
                    /* WARNING: Could not emulate address calculation at 0x0001027a7870 */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)((ulong)*(ushort *)((ulong)uVar1 * 2 + 0x103c117b0) * 4 + 0x1027a787c))();
    return;
  }
  param_1[1] = uRam00000001037c4eb8;
  *param_1 = uVar2;
  param_1[3] = 0;
  param_1[4] = 0;
  param_1[2] = 0;
  if (*_DAT_103fed798 != lVar3) {
    uVar2 = func_0x0001034627dc();
    func_0x00010345b87c(uVar2);
                    /* WARNING: Bad instruction - Truncating control flow here */
    halt_baddata();
  }
  return;
}

