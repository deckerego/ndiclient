/*
 * Development aid: advertises an NDI source named "ndiclient test pattern"
 * that sends 1080p30 UYVY color bars with a moving white bar.
 * Build with tools/build_test_pattern_sender.sh.
 */
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <Processing.NDI.Lib.h>

enum { WIDTH = 1920, HEIGHT = 1080, BYTES_PER_PIXEL = 2, FRAME_RATE = 30, BAR_COUNT = 8, MOVING_BAR_WIDTH = 32 };

/* BT.709 limited-range U, Y, V for white, yellow, cyan, green, magenta, red, blue, black. */
static const uint8_t BAR_COLORS[BAR_COUNT][3] = {
    {128, 235, 128}, {16, 219, 138}, {166, 188, 16}, {54, 173, 34},
    {202, 78, 222}, {90, 63, 240}, {240, 32, 118}, {128, 16, 128},
};

static volatile sig_atomic_t keep_running = 1;

static void stop_running(int signal_number) {
    (void)signal_number;
    keep_running = 0;
}

static void put_pixel_pair(uint8_t *pair, const uint8_t uyv[3]) {
    pair[0] = uyv[0];
    pair[1] = uyv[1];
    pair[2] = uyv[2];
    pair[3] = uyv[1];
}

static void draw_frame(uint8_t *frame, int moving_bar_x) {
    static const uint8_t white[3] = {128, 235, 128};
    const int row_bytes = WIDTH * BYTES_PER_PIXEL;
    for (int x = 0; x < WIDTH; x += 2) {
        const bool on_moving_bar = x >= moving_bar_x && x < moving_bar_x + MOVING_BAR_WIDTH;
        const uint8_t *color = on_moving_bar ? white : BAR_COLORS[x * BAR_COUNT / WIDTH];
        put_pixel_pair(frame + x * BYTES_PER_PIXEL, color);
    }
    for (int y = 1; y < HEIGHT; y++)
        memcpy(frame + y * row_bytes, frame, row_bytes);
}

int main(void) {
    if (!NDIlib_initialize()) {
        fprintf(stderr, "NDI failed to initialize\n");
        return 1;
    }
    signal(SIGINT, stop_running);
    signal(SIGTERM, stop_running);

    NDIlib_send_create_t settings = {.p_ndi_name = "ndiclient test pattern", .clock_video = true};
    NDIlib_send_instance_t sender = NDIlib_send_create(&settings);
    uint8_t *pixels = malloc(WIDTH * HEIGHT * BYTES_PER_PIXEL);

    NDIlib_video_frame_v2_t frame = {
        .xres = WIDTH, .yres = HEIGHT, .FourCC = NDIlib_FourCC_video_type_UYVY,
        .frame_rate_N = FRAME_RATE, .frame_rate_D = 1,
        .frame_format_type = NDIlib_frame_format_type_progressive,
        .timecode = NDIlib_send_timecode_synthesize,
        .p_data = pixels, .line_stride_in_bytes = WIDTH * BYTES_PER_PIXEL,
    };
    for (int frame_number = 0; keep_running; frame_number++) {
        draw_frame(pixels, (frame_number * 16) % WIDTH);
        NDIlib_send_send_video_v2(sender, &frame);
    }

    NDIlib_send_destroy(sender);
    free(pixels);
    NDIlib_destroy();
    return 0;
}
