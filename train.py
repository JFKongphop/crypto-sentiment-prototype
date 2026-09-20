#!/usr/bin/env python3
"""Train a classifier: Trump post -> ETH goes UP or DOWN.

Features: TF-IDF of the post text + VADER sentiment + engagement/time/keyword.
Split is TIME-BASED (train on older posts, test on the most recent) so the score
reflects real forward prediction, not look-ahead leakage.

Usage:
  ./.venv/bin/python train.py                 # 1h label, all posts
  ./.venv/bin/python train.py --label 24h --crypto-only
"""

import argparse
import csv
import numpy as np
from scipy.sparse import hstack, csr_matrix
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, classification_report, confusion_matrix
from vaderSentiment.vaderSentiment import SentimentIntensityAnalyzer
import joblib

DATA = "csv/dataset.csv"
NUM_FEATS = ["word_count", "favorite_count", "repost_count", "hour_utc",
             "weekday", "has_crypto_kw", "has_econ_kw", "sentiment"]


def load(label_key: str, crypto_only: bool):
    vader = SentimentIntensityAnalyzer()
    texts, nums, labels, dates = [], [], [], []
    with open(DATA) as f:
        for r in csv.DictReader(f):
            lab = r[f"label_{label_key}"]
            if lab not in ("UP", "DOWN"):
                continue
            if crypto_only and r["has_crypto_kw"] != "1" and r["has_econ_kw"] != "1":
                continue
            text = r["text"] or ""
            sent = vader.polarity_scores(text)["compound"]
            texts.append(text)
            nums.append([
                float(r["word_count"]), float(r["favorite_count"]),
                float(r["repost_count"]), float(r["hour_utc"]),
                float(r["weekday"]), float(r["has_crypto_kw"]),
                float(r["has_econ_kw"]), sent,
            ])
            labels.append(1 if lab == "UP" else 0)
            dates.append(r["date"])
    return texts, np.array(nums), np.array(labels), dates


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", default="1h", choices=["1h", "24h"])
    ap.add_argument("--crypto-only", action="store_true")
    a = ap.parse_args()

    print(f"Loading (label={a.label}, crypto_only={a.crypto_only})…")
    texts, nums, y, dates = load(a.label, a.crypto_only)
    order = np.argsort(dates)  # chronological
    texts = [texts[i] for i in order]
    nums, y = nums[order], y[order]

    n = len(y)
    cut = int(n * 0.8)
    print(f"rows: {n} | train: {cut} | test: {n - cut}")
    print(f"class balance (all): UP={y.mean():.3f}")

    # TF-IDF fit on train only
    tfidf = TfidfVectorizer(max_features=3000, ngram_range=(1, 2),
                            min_df=5, stop_words="english")
    Xtr_txt = tfidf.fit_transform(texts[:cut])
    Xte_txt = tfidf.transform(texts[cut:])

    # scale numeric features (simple standardization on train stats)
    mu, sd = nums[:cut].mean(0), nums[:cut].std(0) + 1e-9
    Xtr = hstack([Xtr_txt, csr_matrix((nums[:cut] - mu) / sd)])
    Xte = hstack([Xte_txt, csr_matrix((nums[cut:] - mu) / sd)])
    ytr, yte = y[:cut], y[cut:]

    clf = LogisticRegression(max_iter=1000, C=1.0)
    clf.fit(Xtr, ytr)
    pred = clf.predict(Xte)

    majority = max(yte.mean(), 1 - yte.mean())
    acc = accuracy_score(yte, pred)
    print("\n=== RESULTS (test = most recent 20%) ===")
    print(f"majority-class baseline: {majority:.4f}")
    print(f"model accuracy:          {acc:.4f}   ({'+' if acc>majority else ''}{acc-majority:+.4f} vs baseline)")
    print("\n", classification_report(yte, pred, target_names=["DOWN", "UP"]))
    print("confusion matrix [rows=true DOWN,UP][cols=pred DOWN,UP]:")
    print(confusion_matrix(yte, pred))

    # most predictive words
    names = np.array(tfidf.get_feature_names_out())
    coef = clf.coef_[0][:len(names)]
    top_up = names[np.argsort(coef)[-12:]][::-1]
    top_dn = names[np.argsort(coef)[:12]]
    print("\ntop words -> UP :", ", ".join(top_up))
    print("top words -> DOWN:", ", ".join(top_dn))

    joblib.dump({"clf": clf, "tfidf": tfidf, "mu": mu, "sd": sd,
                 "num_feats": NUM_FEATS, "label": a.label}, "model.joblib")
    print("\nSaved model -> model.joblib")


if __name__ == "__main__":
    main()
