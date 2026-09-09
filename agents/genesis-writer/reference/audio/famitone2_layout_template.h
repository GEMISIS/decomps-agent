/* audio_data_layout.h - extractor-provided relocation constants for the music
 * and sound-effect data. The sound engine's data (a FamiTone2-format block)
 * contains absolute 16-bit pointers valid at the addresses the data occupied
 * in the ROM it was extracted from. These constants let the driver map such a
 * pointer onto the extracted asset arrays. They come from the asset manifest
 * (reader side) and describe the extracted files only; nothing here is game
 * logic. TEMPLATE: every value below is a placeholder - fill it from the
 * spec's sound_engine section / asset list and copy to src/audio_data_layout.h.
 */
#ifndef AUDIO_DATA_LAYOUT_H
#define AUDIO_DATA_LAYOUT_H

/* asset holding the song pointer table: its original address, and where the
 * music block header (song count, instrument/sample list pointers, song
 * table) begins inside it. */
#define AUDIO_PTRTABLE_ASSET_BASE   0x0000u   /* placeholder: original address of the pointer-table asset */
#define AUDIO_MUSIC_HEADER_ADDR     0x0000u   /* placeholder: address of the block header */

/* asset holding the instrument records, envelopes, channel streams, the
 * sound-effect pointer list and the effect streams. */
#define AUDIO_STREAMS_ASSET_BASE    0x0000u   /* placeholder: original address of the streams asset */
#define AUDIO_STREAMS_ASSET_SIZE    0u        /* placeholder: byte count of the streams asset */

/* Sound-effect pointer list inside the streams asset: N effects, each an
 * (NTSC pointer, PAL pointer) pair of little-endian words. The spec's
 * sound_engine section says how many effects exist and where the list sits. */
#define AUDIO_SFX_LIST_ADDR         0x0000u   /* placeholder */
#define AUDIO_SFX_COUNT             0u        /* placeholder */

#endif /* AUDIO_DATA_LAYOUT_H */
