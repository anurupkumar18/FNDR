#!/usr/bin/env python3
"""FastVLM sidecar for FNDR — true vision understanding from screenshots.

Usage:
    python3 fastvlm_runner.py <image_path> [optional_question]

Outputs the model's description to stdout. Falls back gracefully if the model
or required libraries are unavailable.

Model: Apple FastVLM-0.5B (LlavaQwen2ForCausalLM + MobileCLIP vision encoder)
       stored at src-tauri/models/llava-fastvithd_0.5b_stage3/
"""

import sys
import os


# ---------------------------------------------------------------------------
# Prompt helpers
# ---------------------------------------------------------------------------

DEFAULT_QUESTION = (
    "Describe the primary activity or content on the screen in ≤ 15 words. "
    "Focus on what the user is actively working on."
)


def build_prompt(question: str) -> str:
    return f"<image>\nUSER: {question}\nASSISTANT:"


# ---------------------------------------------------------------------------
# Backend: MLX-VLM (fastest on Apple Silicon, preferred)
# ---------------------------------------------------------------------------

def transcribe_with_mlx(image_path: str, question: str) -> str | None:
    """Run FastVLM via mlx-vlm — native Apple Silicon speed."""
    try:
        from mlx_vlm import load, generate  # type: ignore
        from mlx_vlm.prompt_utils import apply_chat_template  # type: ignore
        from mlx_vlm.utils import load_config  # type: ignore

        # Resolve model path relative to this sidecar's location
        sidecar_dir = os.path.dirname(os.path.abspath(__file__))
        model_dir = os.path.join(sidecar_dir, "..", "models", "llava-fastvithd_0.5b_stage3")
        model_dir = os.path.normpath(model_dir)

        if not os.path.isdir(model_dir):
            return None

        model, processor = load(model_dir, trust_remote_code=True)
        config = load_config(model_dir)

        formatted = apply_chat_template(
            processor, config, question, num_images=1
        )
        output = generate(
            model,
            processor,
            image=image_path,
            prompt=formatted,
            max_tokens=80,
            verbose=False,
        )
        text = output.strip() if isinstance(output, str) else str(output).strip()
        return text if text else None
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Backend: HuggingFace transformers (cross-platform fallback)
# ---------------------------------------------------------------------------

def transcribe_with_transformers(image_path: str, question: str) -> str | None:
    """Run FastVLM via transformers — works on any platform with enough RAM."""
    try:
        import torch  # type: ignore
        from PIL import Image  # type: ignore
        from transformers import AutoProcessor, AutoModelForVision2Seq  # type: ignore

        sidecar_dir = os.path.dirname(os.path.abspath(__file__))
        model_dir = os.path.join(sidecar_dir, "..", "models", "llava-fastvithd_0.5b_stage3")
        model_dir = os.path.normpath(model_dir)

        if not os.path.isdir(model_dir):
            return None

        dtype = torch.float16 if torch.cuda.is_available() else torch.float32

        processor = AutoProcessor.from_pretrained(model_dir, trust_remote_code=True)
        model = AutoModelForVision2Seq.from_pretrained(
            model_dir,
            torch_dtype=dtype,
            trust_remote_code=True,
            low_cpu_mem_usage=True,
        )

        # Move to best available device
        device = "cuda" if torch.cuda.is_available() else (
            "mps" if getattr(torch.backends, "mps", None) and torch.backends.mps.is_available()
            else "cpu"
        )
        model = model.to(device)
        model.eval()

        image = Image.open(image_path).convert("RGB")
        prompt_text = build_prompt(question)

        inputs = processor(
            text=prompt_text,
            images=image,
            return_tensors="pt",
        ).to(device)

        with torch.inference_mode():
            output_ids = model.generate(
                **inputs,
                max_new_tokens=80,
                do_sample=False,
            )

        # Decode only the newly generated tokens
        input_len = inputs["input_ids"].shape[1]
        new_ids = output_ids[0][input_len:]
        text = processor.tokenizer.decode(new_ids, skip_special_tokens=True).strip()
        return text if text else None

    except Exception:
        return None


# ---------------------------------------------------------------------------
# Backend: LLaVA-style via llava library (last resort)
# ---------------------------------------------------------------------------

def transcribe_with_llava(image_path: str, question: str) -> str | None:
    """Try the llava Python library as a last resort."""
    try:
        from llava.model.builder import load_pretrained_model  # type: ignore
        from llava.mm_utils import get_model_name_from_path, process_images, tokenizer_image_token  # type: ignore
        from llava.constants import IMAGE_TOKEN_INDEX, DEFAULT_IMAGE_TOKEN  # type: ignore
        from PIL import Image  # type: ignore

        sidecar_dir = os.path.dirname(os.path.abspath(__file__))
        model_dir = os.path.join(sidecar_dir, "..", "models", "llava-fastvithd_0.5b_stage3")
        model_dir = os.path.normpath(model_dir)

        if not os.path.isdir(model_dir):
            return None

        model_name = get_model_name_from_path(model_dir)
        tokenizer, model, image_processor, _ = load_pretrained_model(
            model_dir, None, model_name
        )

        image = Image.open(image_path).convert("RGB")
        images_tensor = process_images([image], image_processor, model.config)
        prompt = DEFAULT_IMAGE_TOKEN + "\n" + question
        input_ids = tokenizer_image_token(
            prompt, tokenizer, IMAGE_TOKEN_INDEX, return_tensors="pt"
        ).unsqueeze(0)

        with __import__("torch").inference_mode():
            output_ids = model.generate(
                input_ids,
                images=images_tensor.half(),
                max_new_tokens=80,
                use_cache=True,
            )

        text = tokenizer.batch_decode(output_ids, skip_special_tokens=True)[0].strip()
        # Strip echoed prompt
        if question in text:
            text = text.split(question)[-1].strip()
        return text if text else None
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> int:
    if len(sys.argv) < 2:
        print("[fastvlm] usage: fastvlm_runner.py <image_path> [question]", file=sys.stderr)
        return 1

    image_path = sys.argv[1]
    question = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_QUESTION

    if not os.path.isfile(image_path):
        print(f"[fastvlm] image not found: {image_path}", file=sys.stderr)
        return 1

    # Try in order: MLX → transformers → llava
    for backend, fn in [
        ("mlx-vlm", transcribe_with_mlx),
        ("transformers", transcribe_with_transformers),
        ("llava", transcribe_with_llava),
    ]:
        result = fn(image_path, question)
        if result:
            print(result, end="")
            return 0

    print("[fastvlm-unavailable]", end="")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
