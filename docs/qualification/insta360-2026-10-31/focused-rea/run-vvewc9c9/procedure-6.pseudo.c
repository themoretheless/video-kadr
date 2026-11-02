/* REA/Ghidra pseudocode observation; not original source or product implementation. */

undefined1  [16]
__ZNK9arvrender17GyroTimeConverter12ToGyroTimeMsEN3ins8IdxTimedEi
          (long param_1,int *param_2,int param_3)

{
  int *piVar1;
  char *pcVar2;
  long lVar3;
  char *pcVar4;
  size_t sVar5;
  undefined8 ****ppppuVar6;
  long *plVar7;
  int iVar8;
  ulong uVar9;
  undefined8 uVar10;
  double dVar11;
  double dVar12;
  undefined1 auVar13 [16];
  double dVar14;
  char local_1a0 [8];
  undefined4 local_198;
  undefined1 local_194;
  char local_189;
  undefined8 ***local_188;
  size_t local_180;
  undefined8 uStack_178;
  undefined8 local_170;
  undefined8 uStack_168;
  undefined8 local_160;
  
  piVar1 = param_2 + 2;
  uVar10 = __ZNK3ins6PwTime2MsEv(piVar1);
  iVar8 = *param_2;
  if (*(long **)(param_1 + 0x50) != (long *)0x0 && iVar8 == 0) {
    uVar10 = (**(code **)(**(long **)(param_1 + 0x50) + 0x20))();
    iVar8 = *param_2;
  }
  if ((ulong)(long)iVar8 <= (*(long *)(param_1 + 0x10) - *(long *)(param_1 + 8) >> 3) - 1U) {
    if (*(char *)(param_1 + 0x94) != '\0') {
      uVar10 = *(undefined8 *)(param_1 + 0x98);
    }
    dVar12 = *(double *)(*(long *)(param_1 + 8) + (long)iVar8 * 8);
    __ZN3ins6PwTimeC1Edx(uVar10,&local_170,9000000);
    *(undefined8 *)(param_2 + 4) = uStack_168;
    *(undefined8 *)piVar1 = local_170;
    *(undefined8 *)(param_2 + 6) = local_160;
    dVar14 = *(double *)(param_1 + 0x20);
    iVar8 = *(int *)(param_1 + 0x90);
    dVar11 = (double)__ZNK3ins6PwTime2MsEv(piVar1);
    if (*(long **)(param_1 + 0x40) == (long *)0x0) {
      if (iVar8 == 1) {
        lVar3 = *(long *)(param_1 + 0x28);
        if (lVar3 != *(long *)(param_1 + 0x30)) {
          uVar9 = (ulong)param_3;
          if ((ulong)(*(long *)(param_1 + 0x30) - lVar3 >> 4) <= uVar9) {
            uVar9 = 0;
          }
          plVar7 = *(long **)(lVar3 + uVar9 * 0x10);
          dVar12 = (double)(**(code **)(*plVar7 + 0x20))(plVar7,param_2);
          goto LAB_10189bb80;
        }
      }
      dVar12 = dVar12 + dVar11;
    }
    else {
      dVar12 = (double)(**(code **)(**(long **)(param_1 + 0x40) + 0x18))(dVar12);
      dVar12 = (double)(**(code **)(**(long **)(param_1 + 0x40) + 0x10))(dVar11 + dVar12);
    }
LAB_10189bb80:
    lVar3 = *(long *)(param_1 + 0x28);
    if (lVar3 != *(long *)(param_1 + 0x30)) {
      uVar9 = (ulong)param_3;
      if ((ulong)(*(long *)(param_1 + 0x30) - lVar3 >> 4) <= uVar9) {
        uVar9 = 0;
      }
      dVar11 = (double)(**(code **)**(undefined8 **)(lVar3 + uVar9 * 0x10))(dVar12);
      dVar14 = dVar14 + dVar11 * -0.5 * 1000.0;
    }
    dVar12 = dVar12 + dVar14;
    goto LAB_10189bbd0;
  }
  pcVar2 = 
  "/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/Source/ARVAlgo/Util/GyroTimeConverter.cc"
  ;
  pcVar4 = _strrchr("/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/Source/ARVAlgo/Util/GyroTimeConverter.cc"
                    ,0x2f);
  if (pcVar4 != (char *)0x0) {
    pcVar2 = pcVar4 + 1;
  }
  sVar5 = _strlen(pcVar2);
  if (0x7ffffffffffffff7 < sVar5) {
                    /* WARNING: Subroutine does not return */
    FUN_100016eb8(&local_188);
  }
  if (sVar5 < 0x17) {
    uStack_178 = CONCAT17((char)sVar5,(undefined7)uStack_178);
    ppppuVar6 = &local_188;
    if (sVar5 != 0) goto LAB_10189ba88;
  }
  else {
    uVar9 = (sVar5 & 0xfffffffffffffff8) + 8;
    if ((sVar5 | 7) != 0x17) {
      uVar9 = sVar5 | 7;
    }
    ppppuVar6 = (undefined8 ****)__Znwm(uVar9 + 1);
    uStack_178 = uVar9 + 1 | 0x8000000000000000;
    local_188 = ppppuVar6;
    local_180 = sVar5;
LAB_10189ba88:
    _memmove(ppppuVar6,pcVar2,sVar5);
  }
  *(undefined1 *)((long)ppppuVar6 + sVar5) = 0;
  local_189 = '\f';
  local_198 = 0x734d656d;
  builtin_strncpy(local_1a0,"ToGyroTi",8);
  local_194 = 0;
  __ZN3ins11LLogMessageC1ERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEES9_ii
            (&local_170,&local_188,local_1a0,0x6c,2);
  FUN_100016bb0(&local_170,"GyroTimeConverter can not get gyro time offset, segment: ",0x39);
  __ZNSt3__113basic_ostreamIcNS_11char_traitsIcEEElsEi(&local_170,*param_2);
  __ZN3ins11LLogMessageD1Ev(&local_170);
  if (local_189 < '\0') {
    __ZdlPv(local_1a0);
  }
  if ((long)uStack_178 < 0) {
    __ZdlPv(local_188);
  }
  dVar12 = 0.0;
LAB_10189bbd0:
  auVar13._8_8_ = 0;
  auVar13._0_8_ = dVar12;
  return auVar13;
}

