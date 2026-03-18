#!/usr/bin/env python3
"""Compute RFC-0110 BigInt probe Merkle root."""

import hashlib

OPS = {'ADD':1,'SUB':2,'MUL':3,'DIV':4,'MOD':5,'SHL':6,'SHR':7,
       'CANONICALIZE':8,'CMP':9,'BITLEN':10,'SERIALIZE':11,'DESERIALIZE':12,'I128_ROUNDTRIP':13}

MAX_U64 = 0xFFFFFFFFFFFFFFFF
MAX_U56 = (1<<56)-1
TRAP = 0xDEADDEADDEADDEAD

# Canonical wire encoding of BigInt(1):
_bigint1_bytes = bytes([0x01,0x00,0x00,0x00,0x01,0x00,0x00,0x00,
                         0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00])
BIGINT1_HASH_REF = ('HASHREF', hashlib.sha256(_bigint1_bytes).digest()[:8])

def is_special(v):
    return isinstance(v, tuple) and v[0] in ('MAX', 'TRAP')

def encode(v, neg=False):
    """Encode value to 8 bytes."""
    # Handle special tuples
    if isinstance(v, tuple):
        if v[0]=='MAX': return MAX_U64.to_bytes(8,'little')
        if v[0]=='TRAP': return TRAP.to_bytes(8,'little')
        if v[0]=='HASHREF': return v[1]  # raw 8 bytes stored directly
    # Handle integers
    if isinstance(v, int):
        if v == 0: return (0).to_bytes(8,'little')
        av = abs(v)
        if av <= MAX_U56: return av.to_bytes(7,'little') + (b'\x80' if neg else b'\x00')
        # Large - hash reference
        n = (av.bit_length()+63)//64
        limbs = [(av>>(64*i))&MAX_U64 for i in range(n)]
        hdr = bytes([1, 0xFF if neg else 0, 0, 0, n, 0, 0, 0])
        return hashlib.sha256(hdr + b''.join(l.to_bytes(8,'little') for l in limbs)).digest()[:8]
    # Handle lists
    if isinstance(v, list):
        n = len(v)
        hdr = bytes([1, 0, 0, 0, n, 0, 0, 0])
        return hashlib.sha256(hdr + b''.join(int(x).to_bytes(8,'little') for x in v)).digest()[:8]
    return b'\x00'*8

def mk_entry(op, a, b):
    opb = OPS[op].to_bytes(8, 'little')
    # Determine sign and extract magnitude, keeping integers as integers
    if isinstance(a, int) and a < 0:
        a_neg, a_mag = True, -a
    else:
        a_neg, a_mag = False, a
    if isinstance(b, int) and b < 0:
        b_neg, b_mag = True, -b
    else:
        b_neg, b_mag = False, b
    return opb + encode(a_mag, a_neg) + encode(b_mag, b_neg)

# All 56 entries - using MAX directly as the tuple ('MAX',)
DATA = [
    (0,'ADD',0,2),(1,'ADD',18446744073709551616,1),(2,'ADD',MAX_U64,1),(3,'ADD',1,-1),(4,'ADD',('MAX',),('MAX',)),
    (5,'SUB',-5,-2),(6,'SUB',5,5),(7,'SUB',0,0),(8,'SUB',1,-1),(9,'SUB',('MAX',),1),
    (10,'MUL',2,3),(11,'MUL',4294967296,4294967296),(12,'MUL',0,1),(13,'MUL',('MAX',),('MAX',)),(14,'MUL',-3,4),(15,'MUL',-2,-3),
    (16,'DIV',10,3),(17,'DIV',100,10),(18,'DIV',('MAX',),1),(19,'DIV',1,('MAX',)),(20,'DIV',340282366920938463463374607431768211456,18446744073709551616),
    (21,'MOD',-7,3),(22,'MOD',10,3),(23,'MOD',('MAX',),3),
    (24,'SHL',1,4095),(25,'SHL',1,64),(26,'SHL',1,1),(27,'SHL',('MAX',),1),
    (28,'SHR',2**4095,1),(29,'SHR',2**4095,4096),(30,'SHR',2**4095,64),(31,'SHR',1,0),
    (32,'CANONICALIZE',[0,0,0],0),(33,'CANONICALIZE',[5,0,0],5),(34,'CANONICALIZE',[0],0),
    (35,'CANONICALIZE',[1,0],1),(36,'CANONICALIZE',[MAX_U64,0,0],MAX_U64),
    (37,'CMP',-5,-3),(38,'CMP',0,1),(39,'CMP',('MAX',),('MAX',)),(40,'CMP',-1,1),(41,'CMP',1,2),
    (42,'I128_ROUNDTRIP',0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF,0),(43,'I128_ROUNDTRIP',-0x80000000000000000000000000000000,0),
    (44,'I128_ROUNDTRIP',0,0),(45,'I128_ROUNDTRIP',1,0),(46,'I128_ROUNDTRIP',-1,0),
    (47,'BITLEN',0,1),(48,'BITLEN',1,1),(49,'BITLEN',('MAX',),4096),(50,'BITLEN',9223372036854775808,64),
    (51,'ADD',('MAX',),1),(52,'ADD',MAX_U64,1),(53,'SUB',0,1),
    (54,'SERIALIZE',1,BIGINT1_HASH_REF),(55,'DESERIALIZE',BIGINT1_HASH_REF,1),
]

hashes = []
for i,op,a,b in DATA:
    try:
        e = mk_entry(op,a,b)
        h = hashlib.sha256(e).digest()
        hashes.append(h)
        print(f"{i:2d} {op:14} OK")
    except Exception as x:
        print(f"{i:2d} {op:14} ERROR: {x}")
        hashes.append(b'\x00'*32)

print(f"\n56 entries -> tree...")
while len(hashes) > 1:
    if len(hashes)%2: hashes.append(hashes[-1])
    hashes = [hashlib.sha256(hashes[i]+hashes[i+1]).digest() for i in range(0,len(hashes),2)]
    print(f"  {len(hashes)} nodes")

print(f"\nMERKLE ROOT: {hashes[0].hex()}")
