#!/usr/bin/env python3
"""Remove background from image using lang-sam (prompt-guided segmentation).

Usage: python3 remove_background.py <input> <output> <prompt>
  prompt: comma-separated terms, e.g. "turkey meat, green leaf, parsley"
  All masks from all terms are unioned together.
"""
import sys
import os
import numpy as np
from PIL import Image

def main():
    if len(sys.argv) < 4:
        print(f"Usage: {sys.argv[0]} <input> <output> <prompt>", file=sys.stderr)
        sys.exit(1)

    input_path = sys.argv[1]
    output_path = sys.argv[2]
    prompt_raw = sys.argv[3]

    prompts = [p.strip() for p in prompt_raw.split(",") if p.strip()]
    if not prompts:
        print("Error: empty prompt", file=sys.stderr)
        sys.exit(1)

    from lang_sam import LangSAM
    model = LangSAM()

    image = Image.open(input_path).convert("RGB")
    combined_mask = np.zeros((image.height, image.width), dtype=np.uint8)

    for prompt in prompts:
        results = model.predict([image], [prompt])
        if len(results) == 0 or len(results[0]["masks"]) == 0:
            continue

        masks = results[0]["masks"]
        scores = results[0]["scores"]

        for j, score in enumerate(scores):
            if float(score) > 0.2:
                m = masks[j]
                if hasattr(m, 'cpu'):
                    m = m.cpu().numpy()
                combined_mask = np.maximum(combined_mask, (m * 255).astype(np.uint8))

    rgba = image.convert("RGBA")
    rgba.putalpha(Image.fromarray(combined_mask, mode="L"))
    rgba.save(output_path)

if __name__ == "__main__":
    main()
