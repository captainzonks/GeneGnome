#!/usr/bin/env python3
# ==============================================================================
# generate_data.py - Synthetic Genetic Data for the Demo
# ==============================================================================
# Description: Writes a synthetic reference panel and one synthetic person's
#              23andMe export, imputed VCFs and score table, plus the merge
#              result expected from them. Every value is random: nothing here
#              describes a real person or a real variant. Standard library only,
#              and the same seed always produces the same data.
# Author: Matt Barham
# Created: 2026-11-26
# Modified: 2026-11-26
# Version: 1.0.0
# ==============================================================================

import argparse
import json
import os
import random
import sqlite3
import struct
import zlib
from pathlib import Path

CHROMOSOMES = range(1, 23)
REFERENCE_SAMPLES = 50
BASES = "ACGT"
# Transition partner of each base; chosen twice as often as a transversion
TRANSITION = {"A": "G", "G": "A", "C": "T", "T": "C"}

TYPED_FRACTION = 0.30  # variants present in the 23andMe export
NO_CALL_FRACTION = 0.02  # of those, how many are "--"
UNIMPUTED_FRACTION = 0.03  # variants absent from the imputed VCFs
LOW_PANEL_R2_FRACTION = 0.05  # dropped by the default quality threshold
LOW_USER_R2_FRACTION = 0.04  # labelled low quality in the results

QUALITY_THRESHOLD = 0.9  # the API's default
LOW_QUALITY_R2 = 0.3  # matches app/src/merge.rs

# BGZF blocks hold at most 64 KiB, compressed or not
BGZF_PAYLOAD = 65280


def bgzf_block(data: bytes) -> bytes:
    compressor = zlib.compressobj(6, zlib.DEFLATED, -15)
    body = compressor.compress(data) + compressor.flush()
    header = struct.pack(
        "<4BI2BH2BHH", 31, 139, 8, 4, 0, 0, 255, 6, 66, 67, 2, len(body) + 25
    )
    return header + body + struct.pack("<II", zlib.crc32(data), len(data))


def write_bgzf(path: Path, text: str) -> None:
    raw = text.encode()
    blocks = [
        bgzf_block(raw[start : start + BGZF_PAYLOAD])
        for start in range(0, len(raw), BGZF_PAYLOAD)
    ]
    # An empty block marks the end of the file
    path.write_bytes(b"".join(blocks) + bgzf_block(b""))


def random_alleles(rng: random.Random) -> tuple[str, str]:
    ref = rng.choice(BASES)
    transversions = [b for b in BASES if b not in (ref, TRANSITION[ref])]
    alt = rng.choices([TRANSITION[ref], *transversions], weights=[4, 1, 1])[0]
    return ref, alt


def random_haplotypes(rng: random.Random, alt_frequency: float) -> tuple[int, int]:
    return (
        int(rng.random() < alt_frequency),
        int(rng.random() < alt_frequency),
    )


def create_panel_database(path: Path) -> sqlite3.Connection:
    path.unlink(missing_ok=True)
    database = sqlite3.connect(path)
    database.executescript("""
        CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT);
        CREATE TABLE reference_variants (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chromosome INTEGER NOT NULL, position INTEGER NOT NULL, rsid TEXT,
            ref_allele TEXT NOT NULL, alt_allele TEXT NOT NULL, phased INTEGER,
            allele_freq REAL, minor_allele_freq REAL, imputation_quality REAL,
            is_typed INTEGER, sample_genotypes TEXT NOT NULL
        );
        CREATE INDEX idx_chr_pos ON reference_variants(chromosome, position);
        INSERT INTO metadata VALUES ('source', 'synthetic demo data');
        INSERT INTO metadata VALUES ('num_samples', '50');
        INSERT INTO metadata VALUES ('build', 'GRCh37/hg19');
        """)
    return database


def vcf_header(chromosome: int) -> list[str]:
    return [
        "##fileformat=VCFv4.2",
        '##INFO=<ID=R2,Number=1,Type=Float,Description="Imputation quality">',
        '##FORMAT=<ID=GT,Number=1,Type=String,Description="Genotype">',
        '##FORMAT=<ID=DS,Number=1,Type=Float,Description="Dosage">',
        f"##contig=<ID={chromosome}>",
        "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tDEMO",
    ]


def generate(vault: Path, inputs: Path, variants: int, seed: int) -> None:
    rng = random.Random(seed)
    database = create_panel_database(vault / "reference_panel.db")
    genome = [
        "# Synthetic data in the 23andMe raw data layout. Not a real person.",
        "# rsid\tchromosome\tposition\tgenotype",
    ]
    expected = []

    for chromosome in CHROMOSOMES:
        vcf = vcf_header(chromosome)
        position = 10_000
        for index in range(variants):
            position += rng.randint(200, 5000)
            rsid = f"demo{chromosome}_{index}"
            ref, alt = random_alleles(rng)
            frequency = rng.uniform(0.02, 0.98)
            panel_r2 = (
                rng.uniform(0.5, 0.89)
                if rng.random() < LOW_PANEL_R2_FRACTION
                else rng.uniform(0.9, 1.0)
            )
            panel = {}
            for sample in range(1, REFERENCE_SAMPLES + 1):
                first, second = random_haplotypes(rng, frequency)
                panel[f"samp{sample}"] = f"{first}|{second}"
            typed = rng.random() < TYPED_FRACTION
            database.execute(
                "INSERT INTO reference_variants (chromosome, position, rsid,"
                " ref_allele, alt_allele, phased, allele_freq,"
                " minor_allele_freq, imputation_quality, is_typed,"
                " sample_genotypes) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
                (
                    chromosome,
                    position,
                    rsid,
                    ref,
                    alt,
                    1,
                    round(frequency, 4),
                    round(min(frequency, 1 - frequency), 4),
                    round(panel_r2, 4),
                    int(typed),
                    json.dumps(panel),
                ),
            )

            # The synthetic person's true genotype
            haplotypes = random_haplotypes(rng, frequency)
            dosage = sum(haplotypes)

            call = None
            if typed:
                if rng.random() < NO_CALL_FRACTION:
                    genome.append(f"{rsid}\t{chromosome}\t{position}\t--")
                else:
                    letters = "".join(alt if h else ref for h in haplotypes)
                    genome.append(f"{rsid}\t{chromosome}\t{position}\t{letters}")
                    call = dosage

            imputed = None
            if rng.random() >= UNIMPUTED_FRACTION:
                user_r2 = round(
                    (
                        rng.uniform(0.05, 0.29)
                        if rng.random() < LOW_USER_R2_FRACTION
                        else rng.uniform(0.6, 1.0)
                    ),
                    3,
                )
                # Imputation noise grows as quality falls
                noise = rng.gauss(0, (1 - user_r2) * 0.4)
                estimate = round(min(2.0, max(0.0, dosage + noise)), 3)
                imputed = (estimate, user_r2)
                vcf.append(
                    f"{chromosome}\t{position}\t{rsid}\t{ref}\t{alt}\t.\tPASS\t"
                    f"R2={user_r2}\tGT:DS\t"
                    f"{haplotypes[0]}|{haplotypes[1]}:{estimate}"
                )

            if round(panel_r2, 4) < QUALITY_THRESHOLD:
                continue
            if call is not None:
                result = (float(call), "Genotyped")
            elif imputed is not None:
                low = imputed[1] < LOW_QUALITY_R2
                result = (imputed[0], "ImputedLowQual" if low else "Imputed")
            else:
                result = (None, "Missing")
            expected.append([chromosome, position, *result])

        write_bgzf(inputs / f"chr{chromosome}.dose.vcf.gz", "\n".join(vcf) + "\n")

    database.commit()
    database.close()
    (inputs / "genome_demo.txt").write_text("\n".join(genome) + "\n")

    labels = ["DEMO_TRAIT_A", "DEMO_TRAIT_B", "DEMO_TRAIT_C"]
    scores = [",".join(["sample", *labels])]
    for sample in range(1, REFERENCE_SAMPLES + 2):
        values = [f"{rng.gauss(0, 1):.4f}" for _ in labels]
        scores.append(",".join([f"samp{sample}", *values]))
    (inputs / "scores.txt").write_text("\n".join(scores) + "\n")

    (inputs / "expected.json").write_text(json.dumps(expected))
    print(f"Generated {len(expected)} expected variants in {inputs}")


def chown_tree(root: Path, owner: str) -> None:
    uid, gid = (int(part) for part in owner.split(":"))
    for path in [root, *root.rglob("*")]:
        os.chown(path, uid, gid)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vault", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--variants", type=int, default=1000, help="per chromosome")
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--vault-owner", help="uid:gid for the vault directory")
    parser.add_argument("--inputs-owner", help="uid:gid for the input files")
    args = parser.parse_args()

    args.vault.mkdir(parents=True, exist_ok=True)
    args.inputs.mkdir(parents=True, exist_ok=True)
    generate(args.vault, args.inputs, args.variants, args.seed)
    if args.vault_owner:
        chown_tree(args.vault, args.vault_owner)
    if args.inputs_owner:
        chown_tree(args.inputs, args.inputs_owner)


if __name__ == "__main__":
    main()
