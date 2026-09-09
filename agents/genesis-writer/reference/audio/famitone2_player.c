/* REFERENCE IMPLEMENTATION — FamiTone2-format player for cc65 (clean room B).
 *
 * Use this file as the starting point for src/audio.c when the spec's
 * sound_engine grammar describes the FamiTone2 data format (Shiru's public
 * domain NES sound library): five channel streams, instrument records with
 * volume/arpeggio/pitch envelopes, a 16-bit tempo accumulator, and one
 * sound-effect stream mixed over the music. It was written from the public
 * driver documentation and verified register-for-register against an
 * emulator trace of a FamiTone2 game.
 *
 * To adapt:
 *   1. Replace the asset names (asset_song_event_streams,
 *      asset_song_pointer_table) with the ids from the spec's assets.
 *   2. Fill famitone2_layout_template.h (copy to src/audio_data_layout.h)
 *      with the base addresses the spec/manifest gives for each extracted
 *      block: the data's pointers are absolute to where the block lived in
 *      the original ROM and are rebased on load.
 *   3. Keep the sfx ids in include/audio.h matching the spec's effect list.
 *   4. Keep the register write ORDER of hw_apu_flush_frame equal to the
 *      spec's "register write order per frame" (below is the FamiTone2
 *      order: $4000,$4002,[$4003],$4004,$4006,[$4007],$4008,$400A,$400B,
 *      $400C,$400E; the pulse high bytes only when they changed).
 *   5. Call audio_tick() from where the spec's tick_behavior says (usually
 *      the NMI handler, after uploads, skipped on lag frames).
 * Tempo words: FamiTone2 stores the PAL tempo word first, NTSC second.
 */
/* audio.c - sound engine driver.
 * behavioral_spec sections: sound_engine, system audio_engine.
 *
 * The game's music and sound effects are stored in the FamiTone2 data
 * format (Shiru's public-domain NES audio library, v1.1x). This file is a
 * from-scratch C player for that format, written against the published
 * driver description: a per-frame sequencer with five channel streams
 * (pulse 1, pulse 2, triangle, noise, DPCM), instrument envelopes, a
 * tempo accumulator, and a single sound-effect stream mixed on top of the
 * music by a loudest-wins rule, with all register values buffered and
 * written once per frame in a fixed order (hw_apu_frame / hw_apu_flush_frame).
 *
 * Data addressing: the extracted assets keep the block's absolute 16-bit
 * pointers. Every pointer read from the data is converted once, when it is
 * loaded, into a C pointer into asset_song_event_streams using the
 * relocation constants in audio_data_layout.h; the per-frame code then
 * only dereferences C pointers.
 *
 * Performance notes (cc65): the per-frame path uses file-scope 8-bit
 * variables (the hottest in zero page), byte arrays indexed by 8-bit
 * indices, table lookups, and no function parameters. Envelopes that can
 * only ever output zero (the driver's dummy envelope and data envelopes of
 * the same shape) are flagged inactive when an instrument is set and are
 * skipped by the per-frame loop; this does not change any output.
 */
#include "audio.h"
#include "nes.h"
#include "assets.h"
#include "audio_data_layout.h"

#ifndef NES_HOST_TEST
#pragma code-name   ("CODE")
#pragma rodata-name ("RODATA")
#pragma bss-name    ("BSS")
#endif

/* ---- engine constants -------------------------------------------------- */
#define FT_CHANNELS      5u
#define FT_ENVELOPES     11u      /* 3 per pulse/triangle channel, 2 for noise */
#define FT_DEFAULT_SPEED 6u

/* ---- APU register indices (relative to $4000) ------------------------- */
#define R_PL1_VOL   0x00u
#define R_PL1_SWEEP 0x01u
#define R_PL2_VOL   0x04u
#define R_PL2_SWEEP 0x05u
#define R_TRI_LIN   0x08u
#define R_NOI_VOL   0x0Cu
#define R_NOI_HI    0x0Fu
#define R_DMC_FREQ  0x10u
#define R_DMC_RAW   0x11u
#define R_DMC_START 0x12u
#define R_DMC_LEN   0x13u
#define R_SND_CHN   0x15u

/* Convert an original 16-bit address inside the streams asset to a pointer. */
#define STREAM_PTR(addr) \
    (asset_song_event_streams + (uint16_t)((uint16_t)(addr) - AUDIO_STREAMS_ASSET_BASE))

/* ---- zero-page working variables (per-frame path) --------------------- */
#ifndef NES_HOST_TEST
#pragma bss-name (push, "ZEROPAGE")
#endif
static const uint8_t *rp;          /* current read pointer into the data */
static uint8_t e;                  /* envelope index */
static uint8_t c;                  /* channel index */
static uint8_t b;                  /* byte read from the data */
static uint8_t tmp;
static uint8_t pos;
static uint8_t lo;
static uint8_t hi;
static uint8_t idx;
#ifndef NES_HOST_TEST
#pragma bss-name (pop)
#endif

/* ---- per-channel state (structure of arrays) -------------------------- */
static const uint8_t *chn_ptr[FT_CHANNELS];
static const uint8_t *chn_ret[FT_CHANNELS];
static uint8_t chn_note[FT_CHANNELS];
static uint8_t chn_inst[FT_CHANNELS];
static uint8_t chn_rep[FT_CHANNELS];
static uint8_t chn_ref_len[FT_CHANNELS];
static uint8_t chn_duty[FT_CHANNELS];

/* ---- envelope state ---------------------------------------------------- */
static const uint8_t *env_ptr[FT_ENVELOPES];
static uint8_t env_on[FT_ENVELOPES];      /* 0 = constant zero, skipped */
static uint8_t env_pos[FT_ENVELOPES];
static uint8_t env_rep[FT_ENVELOPES];
static uint8_t env_val[FT_ENVELOPES];

/* ---- song / tempo ------------------------------------------------------ */
static const uint8_t *inst_base;
static const uint8_t *dpcm_base;
static uint8_t song_speed;         /* 0 = no music; bit 7 = paused */
static uint8_t tempo_step_lo;
static uint8_t tempo_step_hi;
static uint8_t tempo_acc_lo;
static uint8_t tempo_acc_hi;
static uint8_t pulse1_prev;
static uint8_t pulse2_prev;
static uint8_t dpcm_effect;
static uint8_t engine_ready;
static uint8_t new_note;
static uint8_t i_vlo, i_vhi, i_alo, i_ahi, i_plo, i_phi;   /* instrument record */

/* ---- sound effect stream (one) ---------------------------------------- */
static const uint8_t *sfx_ptr;
static uint8_t sfx_on;
static uint8_t sfx_rep;
static uint8_t sfx_off;
static uint8_t sfx_buf[11];

/* NTSC 11-bit period table (rest, then octaves 1-5); from the published
 * FamiTone2 driver, indexed by note number. */
static const uint8_t note_table_lo[64] = {
    0x00,0xad,0x4d,0xf2,0x9d,0x4c,0x00,0xb8,0x74,0x34,0xf7,0xbe,0x88,0x56,0x26,0xf8,
    0xce,0xa5,0x7f,0x5b,0x39,0x19,0xfb,0xde,0xc3,0xaa,0x92,0x7b,0x66,0x52,0x3f,0x2d,
    0x1c,0x0c,0xfd,0xee,0xe1,0xd4,0xc8,0xbd,0xb2,0xa8,0x9f,0x96,0x8d,0x85,0x7e,0x76,
    0x70,0x69,0x63,0x5e,0x58,0x53,0x4f,0x4a,0x46,0x42,0x3e,0x3a,0x37,0x34,0x31,0x2e
};
static const uint8_t note_table_hi[64] = {
    0x00,0x06,0x06,0x05,0x05,0x05,0x05,0x04,0x04,0x04,0x03,0x03,0x03,0x03,0x03,0x02,
    0x02,0x02,0x02,0x02,0x02,0x02,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,0x01,
    0x01,0x01,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
    0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00
};

/* ---- header access (init / song start only) ---------------------------- */
static uint8_t hdr_rd(uint16_t addr)
{
    if (addr >= AUDIO_STREAMS_ASSET_BASE) {
        return asset_song_event_streams[addr - AUDIO_STREAMS_ASSET_BASE];
    }
    return asset_song_pointer_table[addr - AUDIO_PTRTABLE_ASSET_BASE];
}

static uint16_t hdr_rdw(uint16_t addr)
{
    return (uint16_t)(hdr_rd(addr) | ((uint16_t)hdr_rd((uint16_t)(addr + 1u)) << 8));
}

/* ---- engine primitives (rare paths) ------------------------------------ */
static void music_stop(void)
{
    song_speed = 0;
    dpcm_effect = 0;
    for (c = 0; c < FT_CHANNELS; c++) {
        chn_rep[c] = 0;
        chn_inst[c] = 0;
        chn_note[c] = 0;
        chn_ref_len[c] = 0;
        chn_duty[c] = 0x30;
    }
    /* the original driver aliases the pulse period shadows onto two of the
     * channel duty bytes, so resetting the channels also resets them */
    pulse1_prev = 0x30;
    pulse2_prev = 0x30;
    for (e = 0; e < FT_ENVELOPES; e++) {     /* all envelopes -> constant zero */
        env_on[e] = 0;
        env_val[e] = 0;
        env_rep[e] = 0;
        env_pos[e] = 0;
    }
}

static void sfx_clear(void)
{
    sfx_on = 0;
    sfx_rep = 0;
    sfx_off = 0;
    sfx_buf[6] = 0;       /* mute triangle */
    sfx_buf[0] = 0x30;    /* mute pulse 1 */
    sfx_buf[3] = 0x30;    /* mute pulse 2 */
    sfx_buf[9] = 0x30;    /* mute noise */
}

/* Point envelope `e` at original address hi:lo and restart it. An envelope
 * whose data is "value 0, loop to start" can only ever output zero; it is
 * flagged inactive so the per-frame loop skips it. */
static void env_set(void)
{
    rp = STREAM_PTR((uint16_t)lo | ((uint16_t)hi << 8));
    env_ptr[e] = rp;
    env_rep[e] = 0;
    env_pos[e] = 0;
    if (rp[0] == 0xC0u && rp[1] == 0u && rp[2] == 0u) {
        env_on[e] = 0;
        env_val[e] = 0;
    } else {
        env_on[e] = 1;
    }
}

/* Load channel c's instrument: duty byte + three envelope pointers. The
 * triangle channel keeps no duty (in the original driver that byte is the
 * pulse 1 period shadow). */
static void set_instrument(void)
{
    rp = inst_base + (uint8_t)(chn_inst[c] << 3);
    tmp = rp[0];
    i_vlo = rp[1]; i_vhi = rp[2];
    i_alo = rp[3]; i_ahi = rp[4];
    i_plo = rp[5]; i_phi = rp[6];
    if (c != 2u) chn_duty[c] = tmp;
    e = (uint8_t)(c + c + c);          /* envelope group: 0, 3, 6, 9 */
    lo = i_vlo; hi = i_vhi; env_set();
    e++;
    lo = i_alo; hi = i_ahi; env_set();
    if (c < 3u) {
        e++;
        lo = i_plo; hi = i_phi; env_set();
    }
}

/* Parse one row of channel c's stream; sets new_note when a note starts. */
static void channel_row(void)
{
    new_note = 0;
    tmp = chn_rep[c];
    if (tmp) {
        tmp--;
        chn_rep[c] = tmp;
        return;
    }
    rp = chn_ptr[c];
    for (;;) {
        b = *rp;
        rp++;
        if (b < 0x80u) {                         /* note */
            if (b & 1u) chn_rep[c] = 1;          /* one empty row follows */
            tmp = (uint8_t)(b >> 1);
            chn_note[c] = tmp;
            new_note = 1;
            break;
        }
        b &= 0x7Fu;
        if ((b & 1u) == 0u) {                    /* instrument change */
            tmp = (uint8_t)(b >> 1);
            chn_inst[c] = tmp;
            continue;
        }
        b >>= 1;
        if (b < 0x3Du) {                         /* empty rows */
            chn_rep[c] = b;
            break;
        }
        if (b == 0x3Du) {                        /* speed */
            song_speed = *rp;
            rp++;
            continue;
        }
        if (b == 0x3Eu) {                        /* loop */
            lo = rp[0]; hi = rp[1];
            rp = STREAM_PTR((uint16_t)lo | ((uint16_t)hi << 8));
            continue;
        }
        /* reference: length byte + 16-bit target; return to pointer + 3 */
        chn_ret[c] = rp + 3;
        chn_ref_len[c] = rp[0];
        lo = rp[1]; hi = rp[2];
        rp = STREAM_PTR((uint16_t)lo | ((uint16_t)hi << 8));
    }
    tmp = chn_ref_len[c];
    if (tmp) {
        tmp--;
        chn_ref_len[c] = tmp;
        if (tmp == 0u) {
            chn_ptr[c] = chn_ret[c];
            return;
        }
    }
    chn_ptr[c] = rp;
}

static void sample_stop(void)
{
    hw_apu_write(R_SND_CHN, 0x0F);
}

static void sample_play(void)
{
    tmp = chn_note[4];
    rp = dpcm_base + (uint8_t)(tmp + tmp + tmp);
    hw_apu_write(R_SND_CHN, 0x0F);
    hw_apu_write(R_DMC_START, rp[0]);
    hw_apu_write(R_DMC_LEN, rp[1]);
    hw_apu_write(R_DMC_FREQ, rp[2]);
    hw_apu_write(R_DMC_RAW, 32);
    hw_apu_write(R_SND_CHN, 0x1F);
}

/* ---- per-frame path ---------------------------------------------------- */
static void envelopes_update(void)
{
    for (e = 0; e < FT_ENVELOPES; e++) {
        if (!env_on[e]) continue;
        tmp = env_rep[e];
        if (tmp) {
            tmp--;
            env_rep[e] = tmp;
            if (tmp) continue;      /* reaching zero reads the next byte now */
        }
        rp = env_ptr[e];
        pos = env_pos[e];
        for (;;) {
            b = rp[pos];
            if (b & 0x80u) {                     /* value + 192 */
                tmp = (uint8_t)(b + 64u);
                env_val[e] = tmp;
                pos++;
                break;
            }
            if (b == 0u) {                       /* loop point */
                pos++;
                pos = rp[pos];
                continue;
            }
            pos++;
            env_rep[e] = b;                      /* repeat counter */
            break;
        }
        env_pos[e] = pos;
    }
}

static void sfx_update(void)
{
    tmp = sfx_rep;
    if (tmp) {
        tmp--;
        sfx_rep = tmp;
    }
    if (tmp == 0u) {                             /* a hold that just ended reads now */
        if (!sfx_on) return;                     /* no active effect: no mixing */
        for (;;) {
            b = sfx_ptr[sfx_off];
            if (b & 0x80u) {                     /* register write into the effect buffer */
                sfx_off++;
                tmp = sfx_ptr[sfx_off];
                sfx_off++;
                b &= 0x7Fu;
                sfx_buf[b] = tmp;
                continue;
            }
            if (b == 0u) {                       /* end of effect (still mixed this frame) */
                sfx_on = 0;
                break;
            }
            sfx_off++;
            sfx_rep = b;                         /* frames to hold */
            break;
        }
    }
    /* mix: an effect channel wins when at least as loud as the music */
    tmp = (uint8_t)(hw_apu_frame[0] & 0x0Fu);
    b = (uint8_t)(sfx_buf[0] & 0x0Fu);
    if (b >= tmp) {
        hw_apu_frame[0] = sfx_buf[0]; hw_apu_frame[1] = sfx_buf[1]; hw_apu_frame[2] = sfx_buf[2];
    }
    tmp = (uint8_t)(hw_apu_frame[3] & 0x0Fu);
    b = (uint8_t)(sfx_buf[3] & 0x0Fu);
    if (b >= tmp) {
        hw_apu_frame[3] = sfx_buf[3]; hw_apu_frame[4] = sfx_buf[4]; hw_apu_frame[5] = sfx_buf[5];
    }
    if (sfx_buf[6]) {
        hw_apu_frame[6] = sfx_buf[6]; hw_apu_frame[7] = sfx_buf[7]; hw_apu_frame[8] = sfx_buf[8];
    }
    tmp = (uint8_t)(hw_apu_frame[9] & 0x0Fu);
    b = (uint8_t)(sfx_buf[9] & 0x0Fu);
    if (b >= tmp) {
        hw_apu_frame[9] = sfx_buf[9]; hw_apu_frame[10] = sfx_buf[10];
    }
}

/* ---- public API -------------------------------------------------------- */
void audio_init(void)
{
    music_stop();
    inst_base = STREAM_PTR(hdr_rdw((uint16_t)(AUDIO_MUSIC_HEADER_ADDR + 1u)));
    dpcm_base = STREAM_PTR(hdr_rdw((uint16_t)(AUDIO_MUSIC_HEADER_ADDR + 3u)));
    for (e = 0; e < HW_APU_FRAME_SIZE; e++) hw_apu_frame[e] = 0;
    sfx_clear();
    tempo_step_lo = 0;
    tempo_step_hi = 0;
    tempo_acc_lo = 0;
    tempo_acc_hi = 0;
    /* one-time channel reset, in the driver's register order */
    hw_apu_write(R_SND_CHN, 0x0F);     /* enable channels, stop DMC */
    hw_apu_write(R_TRI_LIN, 0x80);     /* disable triangle length counter */
    hw_apu_write(R_NOI_HI, 0x00);      /* load noise length */
    hw_apu_write(R_PL1_VOL, 0x30);     /* volumes to 0 */
    hw_apu_write(R_PL2_VOL, 0x30);
    hw_apu_write(R_NOI_VOL, 0x30);
    hw_apu_write(R_PL1_SWEEP, 0x08);   /* no sweep */
    hw_apu_write(R_PL2_SWEEP, 0x08);
    engine_ready = 1;
}

void audio_play_music(uint8_t track)
{
    uint16_t entry;
    if (track >= hdr_rd(AUDIO_MUSIC_HEADER_ADDR)) return;
    music_stop();
    entry = (uint16_t)(AUDIO_MUSIC_HEADER_ADDR + 5u + (uint16_t)track * 14u);
    for (c = 0; c < FT_CHANNELS; c++) {
        chn_ptr[c] = STREAM_PTR(hdr_rdw(entry));
        entry += 2u;
    }
    /* two tempo words follow the channel pointers: PAL first, then NTSC */
    tempo_step_lo = hdr_rd((uint16_t)(entry + 2u));
    tempo_step_hi = hdr_rd((uint16_t)(entry + 3u));
    tempo_acc_lo = 0;
    tempo_acc_hi = FT_DEFAULT_SPEED;
    song_speed = FT_DEFAULT_SPEED;
}

void audio_play_sfx(sfx_id_t id)
{
    uint8_t n = (uint8_t)id;
    if (n >= AUDIO_SFX_COUNT) return;
    sfx_clear();
    sfx_ptr = STREAM_PTR(hdr_rdw((uint16_t)(AUDIO_SFX_LIST_ADDR + (uint16_t)n * 4u)));
    sfx_on = 1;
}

void audio_stop(void)
{
    music_stop();
    sfx_clear();
}

void audio_tick(void)
{
    if (!engine_ready) return;

    if (song_speed && (song_speed & 0x80u) == 0u) {
        /* 16-bit tempo accumulator, done in two bytes */
        lo = (uint8_t)(tempo_acc_lo + tempo_step_lo);
        hi = (uint8_t)(tempo_acc_hi + tempo_step_hi);
        if (lo < tempo_acc_lo) hi++;
        tempo_acc_lo = lo;
        if (hi >= song_speed) {                  /* row update */
            hi = (uint8_t)(hi - song_speed);
            tempo_acc_hi = hi;
            c = 0; channel_row(); if (new_note) set_instrument();
            c = 1; channel_row(); if (new_note) set_instrument();
            c = 2; channel_row(); if (new_note) set_instrument();
            c = 3; channel_row(); if (new_note) set_instrument();
            c = 4; channel_row();
            if (new_note) {
                if (chn_note[4] == 0u) {
                    sample_stop();
                } else if (dpcm_effect == 0u || (hw_apu_read(R_SND_CHN) & 0x10u) == 0u) {
                    dpcm_effect = 0;
                    sample_play();
                }
            }
        } else {
            tempo_acc_hi = hi;
        }
        envelopes_update();
    }

    /* pulse 1: envelopes 0 (volume), 1 (arpeggio), 2 (pitch) */
    b = chn_note[0];
    if (b) {
        idx = (uint8_t)((uint8_t)(b + env_val[1]) & 0x3Fu);
        lo = note_table_lo[idx];
        hi = note_table_hi[idx];
        tmp = env_val[2];
        b = (uint8_t)(lo + tmp);
        if (b < lo) hi++;
        if (tmp & 0x80u) hi--;
        hw_apu_frame[1] = b;
        hw_apu_frame[2] = hi;
        hw_apu_frame[0] = (uint8_t)(env_val[0] | chn_duty[0]);
    } else {
        hw_apu_frame[0] = chn_duty[0];
    }
    /* pulse 2: envelopes 3, 4, 5 */
    b = chn_note[1];
    if (b) {
        idx = (uint8_t)((uint8_t)(b + env_val[4]) & 0x3Fu);
        lo = note_table_lo[idx];
        hi = note_table_hi[idx];
        tmp = env_val[5];
        b = (uint8_t)(lo + tmp);
        if (b < lo) hi++;
        if (tmp & 0x80u) hi--;
        hw_apu_frame[4] = b;
        hw_apu_frame[5] = hi;
        hw_apu_frame[3] = (uint8_t)(env_val[3] | chn_duty[1]);
    } else {
        hw_apu_frame[3] = chn_duty[1];
    }
    /* triangle: envelopes 6, 7, 8 */
    b = chn_note[2];
    if (b) {
        idx = (uint8_t)((uint8_t)(b + env_val[7]) & 0x3Fu);
        lo = note_table_lo[idx];
        hi = note_table_hi[idx];
        tmp = env_val[8];
        b = (uint8_t)(lo + tmp);
        if (b < lo) hi++;
        if (tmp & 0x80u) hi--;
        hw_apu_frame[7] = b;
        hw_apu_frame[8] = hi;
        hw_apu_frame[6] = (uint8_t)(env_val[6] | 0x80u);
    } else {
        hw_apu_frame[6] = 0x80;
    }
    /* noise: envelopes 9 (volume), 10 (arpeggio) */
    b = chn_note[3];
    if (b) {
        tmp = (uint8_t)(((uint8_t)(b + env_val[10]) & 0x0Fu) ^ 0x0Fu);
        hw_apu_frame[10] = (uint8_t)(((uint8_t)(chn_duty[3] << 1) & 0x80u) | tmp);
        hw_apu_frame[9] = (uint8_t)(env_val[9] | 0xF0u);
    } else {
        hw_apu_frame[9] = 0xF0;
    }

    sfx_update();

    /* pulse period MSBs are written only when they change */
    tmp = hw_apu_frame[2];
    if (tmp != pulse1_prev) {
        pulse1_prev = tmp;
        hw_apu_frame[11] = 1;
    } else {
        hw_apu_frame[11] = 0;
    }
    tmp = hw_apu_frame[5];
    if (tmp != pulse2_prev) {
        pulse2_prev = tmp;
        hw_apu_frame[12] = 1;
    } else {
        hw_apu_frame[12] = 0;
    }

    hw_apu_flush_frame();
}
