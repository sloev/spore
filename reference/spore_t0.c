/* SPORE Tier-0 reference node in C — parse, address, ID, and verify, with zero
 * dependencies (just the C standard library). Parses a SPORE envelope (hex or
 * text armor), derives the sender's address, recomputes the message ID, and
 * verifies the Ed25519 signature. The smallest useful node for a machine with a
 * C compiler but no Rust/Python, and a cross-language conformance oracle for
 * docs/REBUILD.md.
 *
 *   cc -O2 -o spore_t0 reference/spore_t0.c && ./spore_t0 <hex-or-armor>
 *   echo '~S1.….~' | ./spore_t0
 *
 * SHA-256/512 are the FIPS 180-4 reference; Ed25519 verify is a direct port of
 * the public-domain reference (ed25519.cr.yp.to / RFC 8032), using 256-bit field
 * arithmetic mod 2^255-19. Correct but intentionally simple (and slow) — a
 * reference, not a fast library.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ===================== SHA-256 (FIPS 180-4) ===================== */
typedef struct { uint32_t s[8]; uint64_t n; uint8_t b[64]; size_t k; } sha256;
static uint32_t ror32(uint32_t x, int c) { return (x >> c) | (x << (32 - c)); }
static void sha256_blk(sha256 *h, const uint8_t *p) {
    static const uint32_t K[64] = {
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2};
    uint32_t w[64], a, b, c, d, e, f, g, hh, t1, t2;
    for (int i = 0; i < 16; i++)
        w[i] = (uint32_t)p[i*4]<<24 | (uint32_t)p[i*4+1]<<16 | (uint32_t)p[i*4+2]<<8 | p[i*4+3];
    for (int i = 16; i < 64; i++) {
        uint32_t s0 = ror32(w[i-15],7)^ror32(w[i-15],18)^(w[i-15]>>3);
        uint32_t s1 = ror32(w[i-2],17)^ror32(w[i-2],19)^(w[i-2]>>10);
        w[i] = w[i-16]+s0+w[i-7]+s1;
    }
    a=h->s[0];b=h->s[1];c=h->s[2];d=h->s[3];e=h->s[4];f=h->s[5];g=h->s[6];hh=h->s[7];
    for (int i = 0; i < 64; i++) {
        uint32_t S1=ror32(e,6)^ror32(e,11)^ror32(e,25), ch=(e&f)^(~e&g);
        t1=hh+S1+ch+K[i]+w[i];
        uint32_t S0=ror32(a,2)^ror32(a,13)^ror32(a,22), mj=(a&b)^(a&c)^(b&c);
        t2=S0+mj;
        hh=g;g=f;f=e;e=d+t1;d=c;c=b;b=a;a=t1+t2;
    }
    h->s[0]+=a;h->s[1]+=b;h->s[2]+=c;h->s[3]+=d;h->s[4]+=e;h->s[5]+=f;h->s[6]+=g;h->s[7]+=hh;
}
static void sha256_init(sha256 *h){
    static const uint32_t iv[8]={0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19};
    memcpy(h->s,iv,sizeof iv); h->n=0; h->k=0;
}
static void sha256_up(sha256 *h,const uint8_t *p,size_t n){
    h->n+=n;
    while(n){ size_t t=64-h->k; if(t>n)t=n; memcpy(h->b+h->k,p,t); h->k+=t; p+=t; n-=t; if(h->k==64){sha256_blk(h,h->b);h->k=0;} }
}
static void sha256_fin(sha256 *h,uint8_t out[32]){
    uint64_t bits=h->n*8; uint8_t pad=0x80; sha256_up(h,&pad,1);
    uint8_t z=0; while(h->k!=56) sha256_up(h,&z,1);
    uint8_t L[8]; for(int i=0;i<8;i++)L[i]=(uint8_t)(bits>>(56-8*i)); sha256_up(h,L,8);
    for(int i=0;i<8;i++){out[i*4]=h->s[i]>>24;out[i*4+1]=h->s[i]>>16;out[i*4+2]=h->s[i]>>8;out[i*4+3]=h->s[i];}
}
static void sha256_hash(const uint8_t *p,size_t n,uint8_t out[32]){ sha256 h; sha256_init(&h); sha256_up(&h,p,n); sha256_fin(&h,out); }

/* ===================== SHA-512 (FIPS 180-4) ===================== */
static uint64_t ror64(uint64_t x,int c){return (x>>c)|(x<<(64-c));}
static void sha512_hash(const uint8_t *msg,size_t len,uint8_t out[64]){
    static const uint64_t K[80]={
        0x428a2f98d728ae22ULL,0x7137449123ef65cdULL,0xb5c0fbcfec4d3b2fULL,0xe9b5dba58189dbbcULL,
        0x3956c25bf348b538ULL,0x59f111f1b605d019ULL,0x923f82a4af194f9bULL,0xab1c5ed5da6d8118ULL,
        0xd807aa98a3030242ULL,0x12835b0145706fbeULL,0x243185be4ee4b28cULL,0x550c7dc3d5ffb4e2ULL,
        0x72be5d74f27b896fULL,0x80deb1fe3b1696b1ULL,0x9bdc06a725c71235ULL,0xc19bf174cf692694ULL,
        0xe49b69c19ef14ad2ULL,0xefbe4786384f25e3ULL,0x0fc19dc68b8cd5b5ULL,0x240ca1cc77ac9c65ULL,
        0x2de92c6f592b0275ULL,0x4a7484aa6ea6e483ULL,0x5cb0a9dcbd41fbd4ULL,0x76f988da831153b5ULL,
        0x983e5152ee66dfabULL,0xa831c66d2db43210ULL,0xb00327c898fb213fULL,0xbf597fc7beef0ee4ULL,
        0xc6e00bf33da88fc2ULL,0xd5a79147930aa725ULL,0x06ca6351e003826fULL,0x142929670a0e6e70ULL,
        0x27b70a8546d22ffcULL,0x2e1b21385c26c926ULL,0x4d2c6dfc5ac42aedULL,0x53380d139d95b3dfULL,
        0x650a73548baf63deULL,0x766a0abb3c77b2a8ULL,0x81c2c92e47edaee6ULL,0x92722c851482353bULL,
        0xa2bfe8a14cf10364ULL,0xa81a664bbc423001ULL,0xc24b8b70d0f89791ULL,0xc76c51a30654be30ULL,
        0xd192e819d6ef5218ULL,0xd69906245565a910ULL,0xf40e35855771202aULL,0x106aa07032bbd1b8ULL,
        0x19a4c116b8d2d0c8ULL,0x1e376c085141ab53ULL,0x2748774cdf8eeb99ULL,0x34b0bcb5e19b48a8ULL,
        0x391c0cb3c5c95a63ULL,0x4ed8aa4ae3418acbULL,0x5b9cca4f7763e373ULL,0x682e6ff3d6b2b8a3ULL,
        0x748f82ee5defb2fcULL,0x78a5636f43172f60ULL,0x84c87814a1f0ab72ULL,0x8cc702081a6439ecULL,
        0x90befffa23631e28ULL,0xa4506cebde82bde9ULL,0xbef9a3f7b2c67915ULL,0xc67178f2e372532bULL,
        0xca273eceea26619cULL,0xd186b8c721c0c207ULL,0xeada7dd6cde0eb1eULL,0xf57d4f7fee6ed178ULL,
        0x06f067aa72176fbaULL,0x0a637dc5a2c898a6ULL,0x113f9804bef90daeULL,0x1b710b35131c471bULL,
        0x28db77f523047d84ULL,0x32caab7b40c72493ULL,0x3c9ebe0a15c9bebcULL,0x431d67c49c100d4cULL,
        0x4cc5d4becb3e42b6ULL,0x597f299cfc657e2aULL,0x5fcb6fab3ad6faecULL,0x6c44198c4a475817ULL};
    uint64_t H[8]={0x6a09e667f3bcc908ULL,0xbb67ae8584caa73bULL,0x3c6ef372fe94f82bULL,0xa54ff53a5f1d36f1ULL,
                   0x510e527fade682d1ULL,0x9b05688c2b3e6c1fULL,0x1f83d9abfb41bd6bULL,0x5be0cd19137e2179ULL};
    size_t total = ((len+16)/128 + 1)*128;
    uint8_t *m = calloc(total,1); memcpy(m,msg,len); m[len]=0x80;
    uint64_t bits=(uint64_t)len*8;
    for(int i=0;i<8;i++) m[total-1-i]=(uint8_t)(bits>>(8*i));
    for(size_t off=0;off<total;off+=128){
        uint64_t w[80];
        for(int i=0;i<16;i++){ w[i]=0; for(int j=0;j<8;j++) w[i]=(w[i]<<8)|m[off+i*8+j]; }
        for(int i=16;i<80;i++){
            uint64_t s0=ror64(w[i-15],1)^ror64(w[i-15],8)^(w[i-15]>>7);
            uint64_t s1=ror64(w[i-2],19)^ror64(w[i-2],61)^(w[i-2]>>6);
            w[i]=w[i-16]+s0+w[i-7]+s1;
        }
        uint64_t a=H[0],b=H[1],c=H[2],d=H[3],e=H[4],f=H[5],g=H[6],h=H[7];
        for(int i=0;i<80;i++){
            uint64_t S1=ror64(e,14)^ror64(e,18)^ror64(e,41), ch=(e&f)^(~e&g);
            uint64_t t1=h+S1+ch+K[i]+w[i];
            uint64_t S0=ror64(a,28)^ror64(a,34)^ror64(a,39), mj=(a&b)^(a&c)^(b&c);
            uint64_t t2=S0+mj;
            h=g;g=f;f=e;e=d+t1;d=c;c=b;b=a;a=t1+t2;
        }
        H[0]+=a;H[1]+=b;H[2]+=c;H[3]+=d;H[4]+=e;H[5]+=f;H[6]+=g;H[7]+=h;
    }
    free(m);
    for(int i=0;i<8;i++) for(int j=0;j<8;j++) out[i*8+j]=(uint8_t)(H[i]>>(56-8*j));
}

/* ===================== field arithmetic mod p = 2^255-19 ===================== */
typedef struct { uint64_t v[4]; } fe; /* little-endian, kept < 2^256 */
static const fe P = {{0xffffffffffffffedULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0x7fffffffffffffffULL}};

static int fe_ge_p(const fe *a){
    for(int i=3;i>=0;i--){ if(a->v[i]!=P.v[i]) return a->v[i]>P.v[i]; }
    return 1; /* equal */
}
static void fe_sub_p(fe *a){ /* a -= p (a >= p assumed) */
    unsigned __int128 br=0;
    for(int i=0;i<4;i++){ unsigned __int128 x=(unsigned __int128)a->v[i]-P.v[i]-br; a->v[i]=(uint64_t)x; br=(x>>64)&1; }
}
static void fe_reduce(fe *a){ /* fold bit-255 then conditional subtract p (a < 2^256) */
    for(int r=0;r<2;r++){
        uint64_t top=a->v[3]>>63; a->v[3]&=0x7fffffffffffffffULL;
        unsigned __int128 c=(unsigned __int128)a->v[0]+(unsigned __int128)19*top; a->v[0]=(uint64_t)c; c>>=64;
        for(int i=1;i<4&&c;i++){ c+=a->v[i]; a->v[i]=(uint64_t)c; c>>=64; }
    }
    if(fe_ge_p(a)) fe_sub_p(a);
}
static void fe_add(fe *r,const fe *a,const fe *b){
    unsigned __int128 c=0; for(int i=0;i<4;i++){ c+=(unsigned __int128)a->v[i]+b->v[i]; r->v[i]=(uint64_t)c; c>>=64; }
    /* c (bit256) folds as 38 */
    unsigned __int128 x=(unsigned __int128)r->v[0]+38*(uint64_t)c; r->v[0]=(uint64_t)x; x>>=64;
    for(int i=1;i<4&&x;i++){ x+=r->v[i]; r->v[i]=(uint64_t)x; x>>=64; }
    fe_reduce(r);
}
static void fe_sub(fe *r,const fe *a,const fe *b){
    /* r = a + (2p - b) to stay non-negative; 2p < 2^257 */
    unsigned __int128 br=0; uint64_t t[4];
    for(int i=0;i<4;i++){ unsigned __int128 x=(unsigned __int128)a->v[i]-b->v[i]-br; t[i]=(uint64_t)x; br=(x>>64)&1; }
    for(int i=0;i<4;i++) r->v[i]=t[i];
    if(br){ /* a<b: add p */
        unsigned __int128 c=0; for(int i=0;i<4;i++){ c+=(unsigned __int128)r->v[i]+P.v[i]; r->v[i]=(uint64_t)c; c>>=64; }
    }
    fe_reduce(r);
}
static void fe_mul(fe *r,const fe *a,const fe *b){
    unsigned __int128 t[8]; for(int i=0;i<8;i++)t[i]=0;
    for(int i=0;i<4;i++){
        unsigned __int128 carry=0;
        for(int j=0;j<4;j++){
            unsigned __int128 cur=t[i+j]+(unsigned __int128)a->v[i]*b->v[j]+carry;
            t[i+j]=(uint64_t)cur; carry=cur>>64;
        }
        t[i+4]+=carry;
    }
    uint64_t lo[4],hi[4];
    for(int i=0;i<4;i++){lo[i]=(uint64_t)t[i];hi[i]=(uint64_t)t[i+4];}
    unsigned __int128 c=0;
    for(int i=0;i<4;i++){ unsigned __int128 x=(unsigned __int128)lo[i]+(unsigned __int128)38*hi[i]+c; r->v[i]=(uint64_t)x; c=x>>64; }
    unsigned __int128 x=(unsigned __int128)r->v[0]+38*(uint64_t)c; r->v[0]=(uint64_t)x; x>>=64;
    for(int i=1;i<4&&x;i++){ x+=r->v[i]; r->v[i]=(uint64_t)x; x>>=64; }
    fe_reduce(r);
}
static void fe_set(fe *r,uint64_t x){ r->v[0]=x; r->v[1]=r->v[2]=r->v[3]=0; }
static int fe_eq(const fe *a,const fe *b){ return !memcmp(a->v,b->v,sizeof a->v); }
static int fe_odd(const fe *a){ return (int)(a->v[0]&1); }
static void fe_frombytes(fe *r,const uint8_t s[32]){ /* little-endian, mask top bit */
    for(int i=0;i<4;i++){ uint64_t x=0; for(int j=0;j<8;j++) x|=(uint64_t)s[i*8+j]<<(8*j); r->v[i]=x; }
    r->v[3]&=0x7fffffffffffffffULL; fe_reduce(r);
}
static void fe_pow(fe *r,const fe *a,const fe *e){ /* r = a^e mod p, e as fe (< 2^255) */
    fe base=*a, acc; fe_set(&acc,1);
    for(int i=0;i<256;i++){
        if((e->v[i>>6]>>(i&63))&1){ fe t; fe_mul(&t,&acc,&base); acc=t; }
        fe t; fe_mul(&t,&base,&base); base=t;
    }
    *r=acc;
}

/* ===================== Ed25519 group ===================== */
typedef struct { fe x,y; } pt; /* affine; identity = (0,1) */
static fe D, II;                 /* curve constant d, and sqrt(-1) */
static const fe EXP_INV = {{0xffffffffffffffebULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0x7fffffffffffffffULL}}; /* p-2 */
static const fe EXP_SQRT= {{0xfffffffffffffffeULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0x0fffffffffffffffULL}}; /* (p+3)/8 */
static const fe EXP_I   = {{0xfffffffffffffffbULL,0xffffffffffffffffULL,0xffffffffffffffffULL,0x1fffffffffffffffULL}}; /* (p-1)/4 */

static void fe_inv(fe *r,const fe *a){ fe_pow(r,a,&EXP_INV); }
static void xrecover(fe *x,const fe *y){
    fe yy,num,den,t; fe_mul(&yy,y,y);
    fe one; fe_set(&one,1);
    fe_sub(&num,&yy,&one);            /* y^2 - 1 */
    fe_mul(&t,&D,&yy); fe_add(&den,&t,&one); /* d y^2 + 1 */
    fe di; fe_inv(&di,&den); fe xx; fe_mul(&xx,&num,&di);
    fe_pow(x,&xx,&EXP_SQRT);
    fe x2; fe_mul(&x2,x,x); fe diff; fe_sub(&diff,&x2,&xx);
    fe zero; fe_set(&zero,0);
    if(!fe_eq(&diff,&zero)){ fe t2; fe_mul(&t2,x,&II); *x=t2; }
    if(fe_odd(x)){ fe t2; fe_sub(&t2,&P,x); *x=t2; } /* x = p - x makes it even */
}
static void ed_add(pt *r,const pt *p1,const pt *p2){
    /* x3 = (x1 y2 + x2 y1)/(1 + d x1 x2 y1 y2); y3 = (y1 y2 + x1 x2)/(1 - d x1 x2 y1 y2) */
    fe x1y2,x2y1,y1y2,x1x2,dxxyy,t,one; fe_set(&one,1);
    fe_mul(&x1y2,&p1->x,&p2->y); fe_mul(&x2y1,&p2->x,&p1->y);
    fe_mul(&y1y2,&p1->y,&p2->y); fe_mul(&x1x2,&p1->x,&p2->x);
    fe_mul(&t,&x1x2,&y1y2); fe_mul(&dxxyy,&D,&t);
    fe nx,ny,dxa,dxb,inv;
    fe_add(&nx,&x1y2,&x2y1); fe_add(&dxa,&one,&dxxyy); fe_inv(&inv,&dxa); fe_mul(&r->x,&nx,&inv);
    fe_add(&ny,&y1y2,&x1x2); fe_sub(&dxb,&one,&dxxyy); fe_inv(&inv,&dxb); fe_mul(&r->y,&ny,&inv);
}
static void ed_scalar(pt *r,const pt *p,const uint8_t *e,int nbytes){
    pt acc; fe_set(&acc.x,0); fe_set(&acc.y,1); /* identity */
    for(int i=nbytes*8-1;i>=0;i--){
        pt t; ed_add(&t,&acc,&acc); acc=t;               /* double */
        if((e[i>>3]>>(i&7))&1){ ed_add(&t,&acc,p); acc=t; } /* add */
    }
    *r=acc;
}
static int ed_decode(pt *r,const uint8_t s[32]){
    fe_frombytes(&r->y,s);
    xrecover(&r->x,&r->y);
    if(fe_odd(&r->x)!=((s[31]>>7)&1)){ fe t; fe_sub(&t,&P,&r->x); r->x=t; }
    return 1;
}
static void ed_init(void){
    /* d = -121665 * inv(121666); I = 2^((p-1)/4) */
    fe a,b,binv,t; fe_set(&a,121665); fe_set(&b,121666);
    fe_inv(&binv,&b); fe_mul(&t,&a,&binv); fe_sub(&D,&P,&t); /* -a*inv(b) = p - a*inv(b) */
    fe two; fe_set(&two,2); fe_pow(&II,&two,&EXP_I);
}
/* Base point B: By = 4/5, Bx = xrecover(By). */
static void ed_base(pt *B){
    fe four,five,inv; fe_set(&four,4); fe_set(&five,5); fe_inv(&inv,&five); fe_mul(&B->y,&four,&inv);
    xrecover(&B->x,&B->y);
}
/* Verify a detached Ed25519 signature `sig` (64B) over `m` under `pk` (32B). */
static int ed25519_verify(const uint8_t sig[64],const uint8_t *m,size_t mlen,const uint8_t pk[32]){
    pt R,A,B,sB,Ah,rhs;
    if(!ed_decode(&R,sig)) return 0;
    if(!ed_decode(&A,pk)) return 0;
    /* h = SHA-512(R || A || m) as a 512-bit little-endian scalar */
    uint8_t *buf=malloc(64+mlen); memcpy(buf,sig,32); memcpy(buf+32,pk,32); memcpy(buf+64,m,mlen);
    uint8_t h[64]; sha512_hash(buf,64+mlen,h); free(buf);
    ed_base(&B);
    ed_scalar(&sB,&B,sig+32,32);   /* [S] B */
    ed_scalar(&Ah,&A,h,64);        /* [h] A */
    ed_add(&rhs,&R,&Ah);           /* R + [h] A */
    return fe_eq(&sB.x,&rhs.x) && fe_eq(&sB.y,&rhs.y);
}

/* ===================== hex / base32 / envelope ===================== */
static int hexval(int c){ if(c>='0'&&c<='9')return c-'0'; if(c>='a'&&c<='f')return c-'a'+10; if(c>='A'&&c<='F')return c-'A'+10; return -1; }
static size_t unhex(const char *s,uint8_t *out,size_t max){
    size_t n=0; for(;s[0]&&s[1];s+=2){ int a=hexval(s[0]),b=hexval(s[1]); if(a<0||b<0)break; if(n>=max)break; out[n++]=(uint8_t)((a<<4)|b); } return n;
}
static const char *B32="ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
static size_t unb32(const char *s,size_t len,uint8_t *out,size_t max){
    uint32_t buf=0; int bits=0; size_t n=0;
    for(size_t i=0;i<len;i++){ char c=s[i]; if(c=='='||c=='\n'||c==' ')continue; const char*p=strchr(B32,c>='a'&&c<='z'?c-32:c); if(!p)return 0;
        buf=(buf<<5)|(uint32_t)(p-B32); bits+=5; if(bits>=8){ bits-=8; if(n<max) out[n++]=(uint8_t)(buf>>bits); } }
    return n;
}
static void hexdump(const uint8_t *b,size_t n){ for(size_t i=0;i<n;i++) printf("%02x",b[i]); }

int main(int argc,char**argv){
    ed_init();
    char line[8192];
    const char *in;
    if(argc>1) in=argv[1];
    else { size_t n=fread(line,1,sizeof line-1,stdin); line[n]=0; in=line; }
    /* trim whitespace */
    char clean[8192]; size_t cn=0;
    for(const char*p=in; *p && cn<sizeof clean-1; p++) if(*p>' ') clean[cn++]=*p;
    clean[cn]=0;

    uint8_t wire[4096]; size_t wlen;
    if(!strncmp(clean,"~S1.",4)){
        char *body=clean+4; char *end=strrchr(body,'~'); if(end)*end=0;
        char *dot=strrchr(body,'.'); if(!dot){fprintf(stderr,"bad armor\n");return 2;} *dot=0;
        uint8_t ck[8]; unb32(dot+1,strlen(dot+1),ck,sizeof ck);
        wlen=unb32(body,strlen(body),wire,sizeof wire);
        uint8_t d[32]; sha256_hash(wire,wlen,d);
        if(memcmp(d,ck,4)){ fprintf(stderr,"armor checksum mismatch\n"); return 1; }
    } else {
        wlen=unhex(clean,wire,sizeof wire);
    }
    if(wlen<16||wire[0]!=0x01){ fprintf(stderr,"not a SPORE v1 envelope\n"); return 1; }

    uint8_t typ=wire[1],flags=wire[2],hops=wire[3];
    uint32_t expiry=(uint32_t)wire[4]<<24|(uint32_t)wire[5]<<16|(uint32_t)wire[6]<<8|wire[7];
    size_t off=16; const uint8_t *pk=NULL;
    if(flags&2){ if(flags&32) off+=8; else { pk=wire+off; off+=32; } }
    size_t plen=(size_t)wire[off]<<8|wire[off+1]; off+=2;
    const uint8_t *payload=wire+off; off+=plen;

    /* id = SHA-256(wire, hops byte zeroed)[..16] */
    uint8_t tmp[4096]; memcpy(tmp,wire,wlen); tmp[3]=0; uint8_t id[32]; sha256_hash(tmp,wlen,id);

    const char *tn = typ==0?"DATA":typ==1?"INV":typ==2?"WANT":typ==3?"ANNOUNCE":"?";
    printf("type    : %s\n",tn);
    printf("flags   : 0x%02x\n",flags);
    printf("hops    : %u\n",hops);
    printf("expiry  : %u\n",expiry);
    printf("dest    : "); hexdump(wire+8,8); printf("\n");
    if(pk){ uint8_t a[32]; sha256_hash(pk,32,a);
        printf("src key : "); hexdump(pk,32); printf("\n");
        printf("src addr: "); hexdump(a,8); printf("\n"); }
    printf("id      : "); hexdump(id,16); printf("\n");
    printf("payload : %.*s\n",(int)plen,payload);

    if(pk){
        const uint8_t *sig=wire+wlen-64;
        uint8_t pre[4096]; memcpy(pre,wire,wlen-64); pre[3]=0; /* body, hops zeroed, no sig */
        int ok=ed25519_verify(sig,pre,wlen-64,pk);
        printf("signature verifies: %s\n", ok?"True":"False");
    }
    return 0;
}
