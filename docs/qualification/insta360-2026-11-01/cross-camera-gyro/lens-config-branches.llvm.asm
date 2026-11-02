1018b431c:	48 06 e8 d2	mov	x8, #0x4032000000000000
1018b4320:	68 02 00 f9	str	x8, [x19]
1018b4324:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b4328:	08 e1 28 91	add	x8, x8, #0xa38
1018b432c:	08 01 40 f9	ldr	x8, [x8]
1018b4330:	7f 00 00 f1	cmp	x3, #0x0
1018b4334:	08 01 83 9a	csel	x8, x8, x3, eq
1018b4338:	68 0e 00 f9	str	x8, [x19, #0x18]
1018b433c:	28 38 01 51	sub	w8, w1, #0x4e
1018b4340:	1f 25 00 71	cmp	w8, #0x9
1018b4344:	88 01 00 54	b.hi	0x1018b4374
1018b4348:	29 00 80 52	mov	w9, #0x1
1018b434c:	28 21 c8 1a	lsl	w8, w9, w8
1018b4350:	69 60 80 52	mov	w9, #0x303
1018b4354:	1f 01 09 6a	tst	w8, w9
1018b4358:	e0 00 00 54	b.eq	0x1018b4374
1018b435c:	28 78 1f 12	and	w8, w1, #0xfffffffe
1018b4360:	1f 39 01 71	cmp	w8, #0x4e
1018b4364:	00 10 62 1e	fmov	d0, #4.00000000
1018b4368:	01 10 61 1e	fmov	d1, #3.00000000
1018b436c:	20 0c 60 1e	fcsel	d0, d1, d0, eq
1018b4370:	60 02 00 fd	str	d0, [x19]
1018b4374:	9f 32 00 71	cmp	w20, #0xc
1018b4378:	08 01 00 54	b.hi	0x1018b4398
1018b437c:	e8 03 14 2a	mov	w8, w20
1018b4380:	49 13 01 b0	adrp	x9, 8809 ; 0x103b1d000
1018b4384:	29 81 19 91	add	x9, x9, #0x660
1018b4388:	8a 00 00 10	adr	x10, #16
1018b438c:	2b 69 68 38	ldrb	w11, [x9, x8]
1018b4390:	4a 09 0b 8b	add	x10, x10, x11, lsl #2
1018b4394:	40 01 1f d6	br	x10
1018b4398:	b3 25 01 f0	adrp	x19, 9399 ; 0x103d6b000
1018b439c:	73 ba 1a 91	add	x19, x19, #0x6ae ; literal pool for: "/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/Source/ARVAlgo/Util/OffsetUtil.cc"
1018b43a0:	e0 03 13 aa	mov	x0, x19
1018b43a4:	e1 05 80 52	mov	w1, #0x2f
1018b43a8:	02 be 6e 94	bl	0x103463bb0 ; symbol stub for: _strrchr
1018b43ac:	1f 00 00 f1	cmp	x0, #0x0
1018b43b0:	75 06 80 9a	csinc	x21, x19, x0, eq
1018b43b4:	e0 03 15 aa	mov	x0, x21
1018b43b8:	e9 bd 6e 94	bl	0x103463b5c ; symbol stub for: _strlen
1018b43bc:	08 01 80 92	mov	x8, #-0x9
1018b43c0:	e8 ff ef f2	movk	x8, #0x7fff, lsl #48
1018b43c4:	1f 00 08 eb	cmp	x0, x8
1018b43c8:	08 5c 00 54	b.hi	0x1018b4f48
1018b43cc:	f3 03 00 aa	mov	x19, x0
1018b43d0:	1f 5c 00 f1	cmp	x0, #0x17
1018b43d4:	e2 41 00 54	b.hs	0x1018b4c10
1018b43d8:	f3 bf 00 39	strb	w19, [sp, #0x2f]
1018b43dc:	f6 63 00 91	add	x22, sp, #0x18
1018b43e0:	13 43 00 b5	cbnz	x19, 0x1018b4c40
1018b43e4:	1b 02 00 14	b	0x1018b4c50
1018b43e8:	88 06 e8 d2	mov	x8, #0x4034000000000000
1018b43ec:	68 02 00 f9	str	x8, [x19]
1018b43f0:	28 00 80 52	mov	w8, #0x1
1018b43f4:	68 22 00 39	strb	w8, [x19, #0x8]
1018b43f8:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b43fc:	08 01 2a 91	add	x8, x8, #0xa80
1018b4400:	08 01 40 f9	ldr	x8, [x8]
1018b4404:	68 0e 00 f9	str	x8, [x19, #0x18]
1018b4408:	9f 12 00 71	cmp	w20, #0x4
1018b452c:	48 06 e8 d2	mov	x8, #0x4032000000000000
1018b4530:	68 02 00 f9	str	x8, [x19]
1018b4534:	28 12 80 52	mov	w8, #0x91
1018b4538:	68 12 00 b9	str	w8, [x19, #0x10]
1018b453c:	14 35 00 34	cbz	w20, 0x1018b4bdc
1018b4540:	9f 06 00 71	cmp	w20, #0x1
1018b4544:	e0 34 00 54	b.eq	0x1018b4be0
1018b4548:	9f 12 00 71	cmp	w20, #0x4
1018b454c:	a0 34 00 54	b.eq	0x1018b4be0
1018b4550:	b3 25 01 f0	adrp	x19, 9399 ; 0x103d6b000
1018b4554:	73 ba 1a 91	add	x19, x19, #0x6ae ; literal pool for: "/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/Source/ARVAlgo/Util/OffsetUtil.cc"
1018b4558:	e0 03 13 aa	mov	x0, x19
1018b455c:	e1 05 80 52	mov	w1, #0x2f
1018b4560:	94 bd 6e 94	bl	0x103463bb0 ; symbol stub for: _strrchr
1018b4564:	1f 00 00 f1	cmp	x0, #0x0
1018b4568:	61 06 80 9a	csinc	x1, x19, x0, eq
1018b456c:	e0 63 00 91	add	x0, sp, #0x18
1018b4570:	e5 73 16 94	bl	0x101e51504
1018b4574:	a1 25 01 f0	adrp	x1, 9399 ; 0x103d6b000
1018b4578:	21 34 27 91	add	x1, x1, #0x9cd ; literal pool for: "GetLensConfig"
1018b457c:	e0 03 00 91	mov	x0, sp
1018b4580:	e1 73 16 94	bl	0x101e51504
1018b4584:	e0 c3 00 91	add	x0, sp, #0x30
1018b4588:	e1 63 00 91	add	x1, sp, #0x18
1018b458c:	e2 03 00 91	mov	x2, sp
1018b4590:	a3 30 80 52	mov	w3, #0x185
1018b4594:	64 00 80 52	mov	w4, #0x3
1018b4598:	c2 dc 9d 97	bl	__ZN3ins11LLogMessageC1ERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEES9_ii
1018b459c:	a1 25 01 f0	adrp	x1, 9399 ; 0x103d6b000
1018b45a0:	21 d8 29 91	add	x1, x1, #0xa76 ; literal pool for: "Unknown X5 lensVersion. lensVersion:"
1018b45a4:	e0 c3 00 91	add	x0, sp, #0x30
1018b45a8:	82 04 80 52	mov	w2, #0x24
1018b45ac:	81 89 9d 97	bl	0x100016bb0
1018b45b0:	e0 c3 00 91	add	x0, sp, #0x30
1018b45b4:	e1 03 14 aa	mov	x1, x20
1018b45b8:	8b b6 6e 94	bl	0x103461fe4 ; symbol stub for: __ZNSt3__113basic_ostreamIcNS_11char_traitsIcEEElsEi
1018b45bc:	bd 01 00 14	b	0x1018b4cb0
1018b45c0:	88 06 e8 d2	mov	x8, #0x4034000000000000
1018b45c4:	68 02 00 f9	str	x8, [x19]
1018b45c8:	20 00 80 52	mov	w0, #0x1
1018b45cc:	60 22 00 39	strb	w0, [x19, #0x8]
1018b45d0:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b45d4:	08 a1 29 91	add	x8, x8, #0xa68
1018b45d8:	08 01 40 f9	ldr	x8, [x8]
1018b45dc:	68 0e 00 f9	str	x8, [x19, #0x18]
1018b45e0:	88 13 80 52	mov	w8, #0x9c
1018b45e4:	77 01 00 14	b	0x1018b4bc0
1018b45e8:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b45ec:	08 a1 2a 91	add	x8, x8, #0xaa8
1018b45f0:	3a 00 00 14	b	0x1018b46d8
1018b45f4:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b45f8:	08 21 28 91	add	x8, x8, #0xa08
1018b45fc:	08 01 40 f9	ldr	x8, [x8]
1018b4600:	4a 01 00 14	b	0x1018b4b28
1018b4604:	48 06 e8 d2	mov	x8, #0x4032000000000000
1018b4608:	68 02 00 f9	str	x8, [x19]
1018b460c:	08 04 80 52	mov	w8, #0x20
1018b4610:	68 12 00 b9	str	w8, [x19, #0x10]
1018b4614:	68 3f 01 90	adrp	x8, 10220 ; 0x1040a0000
1018b4618:	08 c1 29 91	add	x8, x8, #0xa70
