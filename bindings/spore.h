/* Auto-generated from bindings/spec.json — do not edit. */
#ifndef SPORE_H
#define SPORE_H
#include <stdint.h>
#include <stddef.h>

typedef struct { uint8_t *data; size_t len; } SporeBytes;
void spore_bytes_free(SporeBytes b);

void spore_keypair(uint8_t *out_sk, uint8_t *out_pk);
void spore_prekey(uint8_t *out_sec, uint8_t *out_pub);
void spore_addr_of(const uint8_t *pk, uint8_t *out);
void spore_topic_of(const uint8_t *s, size_t s_len, uint8_t *out);
SporeBytes spore_seal(const uint8_t *msg, size_t msg_len, const uint8_t *recip_prekey);
SporeBytes spore_open(const uint8_t *sealed, size_t sealed_len, const uint8_t *prekey_sec);
SporeBytes spore_topic_seal(const uint8_t *msg, size_t msg_len, const uint8_t *psk);
SporeBytes spore_topic_open(const uint8_t *ct, size_t ct_len, const uint8_t *psk);
SporeBytes spore_armor_wrap(const uint8_t *env, size_t env_len);
SporeBytes spore_armor_unwrap(const uint8_t *text, size_t text_len);
SporeBytes spore_message_new(const uint8_t *sk, const uint8_t *dest, uint32_t expiry, const uint8_t *payload, size_t payload_len);
uint8_t spore_message_verify(const uint8_t *wire, size_t wire_len);
uint8_t spore_message_id(const uint8_t *wire, size_t wire_len, uint8_t *out);
SporeBytes spore_message_payload(const uint8_t *wire, size_t wire_len);

#endif
