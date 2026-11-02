/* REA/Ghidra function-only observation; not original source or product code. */

/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

float entry(float *param_1)

{
  float fVar1;
  float fVar2;
  float fVar3;
  float fVar4;
  
  fVar1 = *param_1;
  fVar2 = param_1[1];
  fVar4 = param_1[2];
  fVar3 = param_1[3];
  return (_DAT_104a63dbc *
          ((_DAT_104a63dc0 * fVar3 + fVar1 * _DAT_104a63dcc + fVar4 * _DAT_104a63dc4) -
          fVar2 * _DAT_104a63dc8) +
          _DAT_104a63db0 *
          (((-(_DAT_104a63dc0 * fVar1) + fVar3 * _DAT_104a63dcc) - fVar2 * _DAT_104a63dc4) -
          fVar4 * _DAT_104a63dc8) +
         _DAT_104a63db8 *
         ((_DAT_104a63dc4 * fVar3 + fVar2 * _DAT_104a63dcc + fVar1 * _DAT_104a63dc8) -
         fVar4 * _DAT_104a63dc0)) -
         _DAT_104a63db4 *
         ((_DAT_104a63dc8 * fVar3 + fVar4 * _DAT_104a63dcc + fVar2 * _DAT_104a63dc0) -
         fVar1 * _DAT_104a63dc4);
}

