#include <stdio.h>
#include <string.h>

#include <libavcodec/avcodec.h>

int main(void)
{
    const AVCodec *generic = avcodec_find_decoder(AV_CODEC_ID_EAC3);
    const AVCodec *stock = avcodec_find_decoder_by_name("eac3");
    const AVCodec *openjoc = avcodec_find_decoder_by_name("libopenjoc");

    if (!generic || !stock || !openjoc) {
        fprintf(stderr, "missing decoder: generic=%p stock=%p libopenjoc=%p\n",
                (const void *)generic, (const void *)stock,
                (const void *)openjoc);
        return 1;
    }
    printf("generic_eac3=%s\nstock_eac3=%s\nnamed_openjoc=%s\n",
           generic->name, stock->name, openjoc->name);
    if (strcmp(generic->name, "eac3") || strcmp(stock->name, "eac3") ||
        strcmp(openjoc->name, "libopenjoc") ||
        openjoc->id != AV_CODEC_ID_EAC3)
        return 1;
    return 0;
}
