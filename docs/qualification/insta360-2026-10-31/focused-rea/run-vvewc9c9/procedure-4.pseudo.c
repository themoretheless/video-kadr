/* REA/Ghidra pseudocode observation; not original source or product implementation. */

long __ZN6studio20StabilizationService12GetGyroCacheEv(void)

{
  int iVar1;
  long lVar2;
  undefined ***pppuVar3;
  long lVar4;
  long lVar5;
  int *local_90 [3];
  undefined **local_78 [3];
  undefined ***local_60;
  undefined **local_58 [3];
  undefined ***local_40;
  long local_38;
  
  local_38 = *(long *)PTR____stack_chk_guard_103fed798;
  __ZN6studio14GlobalSettings9SingletonEv();
  __ZNK6studio14GlobalSettings15projectLocationEv(local_90);
  local_58[0] = &PTR_FUN_10400ad60;
  local_40 = local_58;
  lVar2 = __ZN5basic8FileUtil16GetDirectorySizeERK7QStringNSt3__18functionIFb9QFileInfoEEE
                    (local_90,local_58);
  if (local_40 == local_58) {
    lVar4 = 4;
    pppuVar3 = local_58;
LAB_100479438:
    (*(code *)(*pppuVar3)[lVar4])();
  }
  else if (local_40 != (undefined ***)0x0) {
    lVar4 = 5;
    pppuVar3 = local_40;
    goto LAB_100479438;
  }
  if (local_90[0] != (int *)0x0) {
    LOAcquire();
    iVar1 = *local_90[0];
    *local_90[0] = iVar1 + -1;
    LORelease();
    if (iVar1 == 1) {
      __ZN10QArrayData10deallocateEPS_xx(local_90[0],2,8);
    }
  }
  __ZN6studio4util12PathResolver20GetGyroCacheSavePathEv(local_90);
  local_78[0] = &PTR_FUN_10400ade0;
  local_60 = local_78;
  lVar4 = __ZN5basic8FileUtil16GetDirectorySizeERK7QStringNSt3__18functionIFb9QFileInfoEEE
                    (local_90,local_78);
  if (local_60 == local_78) {
    lVar5 = 4;
    pppuVar3 = local_78;
  }
  else {
    if (local_60 == (undefined ***)0x0) goto LAB_1004794c4;
    lVar5 = 5;
    pppuVar3 = local_60;
  }
  (*(code *)(*pppuVar3)[lVar5])();
LAB_1004794c4:
  if (local_90[0] != (int *)0x0) {
    LOAcquire();
    iVar1 = *local_90[0];
    *local_90[0] = iVar1 + -1;
    LORelease();
    if (iVar1 == 1) {
      __ZN10QArrayData10deallocateEPS_xx(local_90[0],2,8);
    }
  }
  if (*(long *)PTR____stack_chk_guard_103fed798 == local_38) {
    return lVar4 + lVar2;
  }
                    /* WARNING: Subroutine does not return */
  ___stack_chk_fail();
}

