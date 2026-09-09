/* test_audio.c - SHAPE of the host test for the sound engine driver.
 * Every expected value below is a placeholder: take the real ones from the
 * spec's sound_engine section (init sequence, register write order, the
 * per-frame register images of the first frames, the stop behaviour, one
 * complete sound effect). Never invent register values.
 */
#include "runner.h"
#include "audio.h"
#include "nes.h"
#include "hw_stub.h"
#include <string.h>

int g_failures;

/* hw_stub.h records every APU register write as (index, register, value) */
static void expect_write(uint8_t i, uint8_t reg, uint8_t val)
{
    ASSERT(stub_apu_calls > i);
    ASSERT(stub_apu_log[i].reg == reg);
    ASSERT(stub_apu_log[i].val == val);
}

int main(void)
{
    /* 1. init: the spec's "init sequence" register writes, in order */
    stub_apu_reset();
    audio_init();
    ASSERT(stub_apu_calls == 0u /* spec: number of init writes */);
    /* expect_write(0, 0x15, 0x0F); ... from the spec */

    /* 2. first music frames: one register image per frame, in the spec's
     *    "register write order per frame" */
    audio_play_song(0);
    stub_apu_reset();
    audio_tick();
    ASSERT(stub_apu_calls == 0u /* spec: writes on the first tick */);
    /* expect_write(0, 0x00, 0x00); ... from the spec's per-frame image */

    /* 3. stop: the spec's "stopped state" (what is written once, what is
     *    written every frame while silent) */
    audio_stop();
    stub_apu_reset();
    audio_tick();
    ASSERT(stub_apu_calls == 0u /* spec */);

    /* 4. one full sound effect during silence: per-frame images until the
     *    effect ends and the music value returns */
    audio_play_sfx(0 /* spec: first effect id */);
    stub_apu_reset();
    audio_tick();
    ASSERT(stub_apu_calls == 0u /* spec */);

    return g_failures ? 1 : 0;
}
