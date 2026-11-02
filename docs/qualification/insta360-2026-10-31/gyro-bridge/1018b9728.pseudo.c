/* REA/Ghidra function-only observation; not original source or product code. */

/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(undefined1 param_1 [16],undefined1 param_2 [16],float param_3,float param_4)

{
  float fVar1;
  undefined8 uVar2;
  undefined8 local_60;
  undefined8 uStack_58;
  undefined8 uStack_50;
  undefined8 uStack_48;
  undefined8 local_40;
  undefined4 local_38;
  undefined8 local_34;
  undefined8 local_2c;
  undefined4 local_24;
  
  func_0x0001034626b0(0x100017b6c,0x104a63da0,0x100000000);
  uStack_58 = uRam0000000103b1d988;
  local_60 = _DAT_103b1d980;
  uStack_48 = uRam0000000103b1d998;
  uStack_50 = _DAT_103b1d990;
  local_40 = _DAT_103b1d9a0;
  local_38 = 0xb33bbd2e;
  local_2c = 0;
  local_34 = 0;
  local_24 = 0x3f800000;
  uVar2 = _DAT_103b1d990;
  _DAT_104a63db0 = (float)func_0x000101036b60(&local_60);
  _DAT_104a63db4 = (float)uVar2;
  _DAT_104a63db8 = param_3;
  _DAT_104a63dbc = param_4;
  fVar1 = _DAT_104a63db0 * _DAT_104a63db0 + _DAT_104a63db4 * _DAT_104a63db4 +
          param_3 * param_3 + param_4 * param_4;
  uRam0000000104a63dc8 = CONCAT44(param_4 / fVar1,-param_3 / fVar1);
  _DAT_104a63dc0 = CONCAT44(-_DAT_104a63db4 / fVar1,-_DAT_104a63db0 / fVar1);
  return;
}

