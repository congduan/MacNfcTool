#include <nfc/nfc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MF_AUTH_A 0x60
#define MF_AUTH_B 0x61
#define MF_READ 0x30
#define MF_WRITE 0xA0
#define MF_ACK 0x0A

typedef struct {
  nfc_context *context;
  nfc_device *device;
  uint8_t uid[10];
  size_t uid_len;
  bool has_target;
} nfc_bridge_handle;

static void set_error(char *err, size_t err_len, const char *message) {
  if (err == NULL || err_len == 0) {
    return;
  }
  if (message == NULL) {
    message = "unknown error";
  }
  snprintf(err, err_len, "%s", message);
}

static void to_hex(const uint8_t *data, size_t data_len, char *out, size_t out_len) {
  if (out == NULL || out_len == 0) {
    return;
  }
  size_t pos = 0;
  for (size_t i = 0; i < data_len && pos + 2 < out_len; ++i) {
    int written = snprintf(out + pos, out_len - pos, "%02X", data[i]);
    if (written <= 0) {
      break;
    }
    pos += (size_t)written;
  }
  out[pos < out_len ? pos : (out_len - 1)] = '\0';
}

static int mifare_authenticate(nfc_device *device, uint8_t block, const uint8_t *key,
                               const uint8_t *uid4, uint8_t key_type, char *err,
                               size_t err_len) {
  uint8_t cmd[12];
  cmd[0] = key_type == 0 ? MF_AUTH_A : MF_AUTH_B;
  cmd[1] = block;
  memcpy(cmd + 2, key, 6);
  memcpy(cmd + 8, uid4, 4);

  uint8_t rx[18];
  int ret = nfc_initiator_transceive_bytes(device, cmd, sizeof(cmd), rx, sizeof(rx), 0);
  if (ret < 0) {
    set_error(err, err_len, nfc_strerror(device));
    return -1;
  }
  return 0;
}

static int mifare_read_block(nfc_device *device, uint8_t block, uint8_t *out16, char *err,
                             size_t err_len) {
  uint8_t cmd[2] = {MF_READ, block};
  uint8_t rx[18];
  int ret = nfc_initiator_transceive_bytes(device, cmd, sizeof(cmd), rx, sizeof(rx), 0);
  if (ret < 0) {
    set_error(err, err_len, nfc_strerror(device));
    return -1;
  }
  if (ret < 16) {
    set_error(err, err_len, "short read response from card");
    return -1;
  }
  memcpy(out16, rx, 16);
  return 0;
}

static int mifare_write_block(nfc_device *device, uint8_t block, const uint8_t *data16, char *err,
                              size_t err_len) {
  uint8_t cmd[2] = {MF_WRITE, block};
  uint8_t ack[4];

  int ret = nfc_initiator_transceive_bytes(device, cmd, sizeof(cmd), ack, sizeof(ack), 0);
  if (ret < 0) {
    set_error(err, err_len, nfc_strerror(device));
    return -1;
  }
  if (ret < 1 || ack[0] != MF_ACK) {
    set_error(err, err_len, "card rejected write command");
    return -1;
  }

  ret = nfc_initiator_transceive_bytes(device, data16, 16, ack, sizeof(ack), 0);
  if (ret < 0) {
    set_error(err, err_len, nfc_strerror(device));
    return -1;
  }
  if (ret < 1 || ack[0] != MF_ACK) {
    set_error(err, err_len, "card rejected write payload");
    return -1;
  }
  return 0;
}

int nfc_bridge_connect(nfc_bridge_handle **out, char *err, size_t err_len) {
  if (out == NULL) {
    set_error(err, err_len, "out pointer is null");
    return -1;
  }

  *out = NULL;
  nfc_context *ctx = NULL;
  nfc_init(&ctx);
  if (ctx == NULL) {
    set_error(err, err_len, "nfc_init failed");
    return -1;
  }

  nfc_connstring connstrings[8];
  size_t device_count = nfc_list_devices(ctx, connstrings, 8);

  nfc_device *dev = nfc_open(ctx, NULL);
  if (dev == NULL) {
    if (device_count == 0) {
      set_error(err, err_len,
                "no NFC device found (libnfc scan=0). Try setting "
                "LIBNFC_AUTO_SCAN=true and LIBNFC_INTRUSIVE_SCAN=true, "
                "or set LIBNFC_DEVICE=pn532_uart:/dev/tty.xxx");
    } else {
      char msg[256];
      snprintf(msg, sizeof(msg), "nfc_open failed, but scan found %zu device(s), first=%s",
               device_count, connstrings[0]);
      set_error(err, err_len, msg);
    }
    nfc_exit(ctx);
    return -1;
  }

  if (nfc_initiator_init(dev) < 0) {
    const char *last_err = nfc_strerror(dev);
    nfc_close(dev);
    nfc_exit(ctx);
    set_error(err, err_len, last_err);
    return -1;
  }

  nfc_bridge_handle *handle = (nfc_bridge_handle *)calloc(1, sizeof(nfc_bridge_handle));
  if (handle == NULL) {
    nfc_close(dev);
    nfc_exit(ctx);
    set_error(err, err_len, "failed to allocate nfc handle");
    return -1;
  }

  handle->context = ctx;
  handle->device = dev;
  handle->has_target = false;
  *out = handle;
  return 0;
}

int nfc_bridge_probe(size_t *count_out, char *first_connstring, size_t first_connstring_len,
                     char *err, size_t err_len) {
  if (count_out == NULL) {
    set_error(err, err_len, "count_out is null");
    return -1;
  }
  *count_out = 0;
  if (first_connstring != NULL && first_connstring_len > 0) {
    first_connstring[0] = '\0';
  }

  nfc_context *ctx = NULL;
  nfc_init(&ctx);
  if (ctx == NULL) {
    set_error(err, err_len, "nfc_init failed");
    return -1;
  }

  nfc_connstring connstrings[8];
  size_t count = nfc_list_devices(ctx, connstrings, 8);
  *count_out = count;
  if (count > 0 && first_connstring != NULL && first_connstring_len > 0) {
    snprintf(first_connstring, first_connstring_len, "%s", connstrings[0]);
  }
  nfc_exit(ctx);
  return 0;
}

void nfc_bridge_disconnect(nfc_bridge_handle *handle) {
  if (handle == NULL) {
    return;
  }
  if (handle->device != NULL) {
    nfc_close(handle->device);
  }
  if (handle->context != NULL) {
    nfc_exit(handle->context);
  }
  free(handle);
}

int nfc_bridge_get_device_name(nfc_bridge_handle *handle, char *out, size_t out_len, char *err,
                               size_t err_len) {
  if (handle == NULL || handle->device == NULL) {
    set_error(err, err_len, "reader not connected");
    return -1;
  }
  const char *name = nfc_device_get_name(handle->device);
  if (name == NULL) {
    set_error(err, err_len, "unable to get device name");
    return -1;
  }
  snprintf(out, out_len, "%s", name);
  return 0;
}

static const char *card_type_from_sak(uint8_t sak) {
  switch (sak) {
  case 0x08:
    return "Mifare Classic 1K";
  case 0x18:
    return "Mifare Classic 4K";
  case 0x00:
    return "Mifare Ultralight or NTAG";
  default:
    return "ISO14443-A (unknown subtype)";
  }
}

int nfc_bridge_scan(nfc_bridge_handle *handle, char *uid_hex, size_t uid_len, char *atqa_hex,
                    size_t atqa_len, char *sak_hex, size_t sak_len, char *card_type,
                    size_t card_type_len, char *err, size_t err_len) {
  if (handle == NULL || handle->device == NULL) {
    set_error(err, err_len, "reader not connected");
    return -1;
  }

  const nfc_modulation modulation = {.nmt = NMT_ISO14443A, .nbr = NBR_106};
  nfc_target target;
  memset(&target, 0, sizeof(target));

  int selected = nfc_initiator_select_passive_target(handle->device, modulation, NULL, 0, &target);
  if (selected <= 0) {
    set_error(err, err_len, selected == 0 ? "no card detected" : nfc_strerror(handle->device));
    return -1;
  }

  if (target.nti.nai.szUidLen == 0 || target.nti.nai.szUidLen > sizeof(handle->uid)) {
    set_error(err, err_len, "unsupported UID length");
    return -1;
  }

  handle->uid_len = target.nti.nai.szUidLen;
  memcpy(handle->uid, target.nti.nai.abtUid, handle->uid_len);
  handle->has_target = true;

  to_hex(handle->uid, handle->uid_len, uid_hex, uid_len);
  to_hex(target.nti.nai.abtAtqa, 2, atqa_hex, atqa_len);

  uint8_t sak = target.nti.nai.btSak;
  to_hex(&sak, 1, sak_hex, sak_len);
  snprintf(card_type, card_type_len, "%s", card_type_from_sak(sak));

  return 0;
}

int nfc_bridge_read_sector(nfc_bridge_handle *handle, uint8_t sector, const uint8_t *key,
                           uint8_t key_type, uint8_t *out_data, size_t out_len, char *err,
                           size_t err_len) {
  if (handle == NULL || handle->device == NULL) {
    set_error(err, err_len, "reader not connected");
    return -1;
  }
  if (!handle->has_target) {
    set_error(err, err_len, "no card selected, call scan first");
    return -1;
  }
  if (sector > 15) {
    set_error(err, err_len, "only Mifare Classic 1K sectors 0-15 are supported");
    return -1;
  }
  if (key == NULL || out_data == NULL || out_len < 64) {
    set_error(err, err_len, "invalid read buffer or key");
    return -1;
  }

  const uint8_t first_block = (uint8_t)(sector * 4);

  uint8_t uid4[4] = {0};
  if (handle->uid_len >= 4) {
    memcpy(uid4, handle->uid + (handle->uid_len - 4), 4);
  } else {
    memcpy(uid4, handle->uid, handle->uid_len);
  }
  if (mifare_authenticate(handle->device, first_block, key, uid4, key_type, err, err_len) != 0) {
    return -1;
  }

  for (uint8_t i = 0; i < 4; ++i) {
    if (mifare_read_block(handle->device, (uint8_t)(first_block + i), out_data + (i * 16), err,
                          err_len) != 0) {
      return -1;
    }
  }

  return 0;
}

int nfc_bridge_write_block(nfc_bridge_handle *handle, uint8_t sector, uint8_t block,
                           const uint8_t *data, size_t data_len, const uint8_t *key,
                           uint8_t key_type, char *err, size_t err_len) {
  if (handle == NULL || handle->device == NULL) {
    set_error(err, err_len, "reader not connected");
    return -1;
  }
  if (!handle->has_target) {
    set_error(err, err_len, "no card selected, call scan first");
    return -1;
  }
  if (sector > 15 || block > 3) {
    set_error(err, err_len, "invalid sector/block for Mifare Classic 1K");
    return -1;
  }
  if (data == NULL || data_len != 16 || key == NULL) {
    set_error(err, err_len, "invalid data or key");
    return -1;
  }

  const uint8_t first_block = (uint8_t)(sector * 4);
  const uint8_t abs_block = (uint8_t)(first_block + block);

  uint8_t uid4[4] = {0};
  if (handle->uid_len >= 4) {
    memcpy(uid4, handle->uid + (handle->uid_len - 4), 4);
  } else {
    memcpy(uid4, handle->uid, handle->uid_len);
  }
  if (mifare_authenticate(handle->device, first_block, key, uid4, key_type, err, err_len) != 0) {
    return -1;
  }
  if (mifare_write_block(handle->device, abs_block, data, err, err_len) != 0) {
    return -1;
  }

  return 0;
}
