#!/bin/bash
# Builds Files search's synthetic testbed (ADR 033 part 10) with macOS's own
# tools only: documents, a docx, PDFs (cupsfilter), screenshots with text
# (rendered PDF pages, sips), spoken recordings (say). Nothing personal, no
# downloads. evals/files/queries.json labels what each query should find;
# `make files-eval` indexes this folder with the real model and scores it.
#
# usage: scripts/files/make-testbed.sh [folder]   (default ~/fndr-files-testbed)
set -euo pipefail
OUT="${1:-$HOME/fndr-files-testbed}"
if [[ "$(uname -s)" != Darwin ]]; then
  echo "The testbed uses macOS's say, cupsfilter, textutil and sips." >&2
  exit 1
fi
rm -rf "$OUT"
mkdir -p "$OUT/Documents" "$OUT/Screenshots" "$OUT/Recordings" "$OUT/Papers"
cd "$OUT"

doc() { printf '%s\n' "$2" > "Documents/$1"; }
doc "lease-notes.md" "# Apartment lease

Tenants may keep one cat or one small dog. Any other pets need the landlord's written consent.
The deposit is two months of rent and is returned within 30 days of moving out.
Parking space 14 is included; a second car costs 80 dollars a month."
doc "tokyo-trip.txt" "Flight to Tokyo Haneda on November 3. Then the train to Kamakura to see the Great Buddha,
and back for dinner in Shibuya. Hotel confirmation number HX7Q2L."
doc "sourdough.md" "Sourdough starter: feed it equal weights of flour and water every day.
Bake at 250 degrees with steam for the first twenty minutes for a crisp crust."
doc "offsite-agenda.txt" "Team offsite agenda. Morning: roadmap review and hiring plan. Afternoon: budget for next quarter.
Dinner reservation at the harbour restaurant at seven."
doc "garden.md" "Tomatoes need full sun and deep watering twice a week. Prune the suckers so the plant puts its energy into fruit."
doc "invoice-studio.txt" "Invoice K7Q2LM for the studio rent in March, 1,200 euros, due on the 15th."
doc "marathon.md" "Training plan for the spring marathon: long runs on Sundays, intervals on Tuesdays, rest on Fridays."
doc "bengali-recipe.txt" "ইলিশ মাছ রান্না: সরষে বাটা দিয়ে ইলিশ মাছ ভাপে রান্না করুন।"
doc "arabic-library.txt" "تغلق المكتبة العامة في الساعة الثامنة مساء كل يوم ما عدا الجمعة."
for n in $(seq 1 25); do
  doc "meeting-$n.txt" "Weekly sync number $n. Status updates on the design system, the release train and support tickets."
done
textutil -convert docx -output "Documents/insurance-policy.docx" /dev/stdin <<< "Car insurance policy. Windscreen damage is covered without an excess. Claims must be filed within 14 days."

page() { printf '%s' "$2" > "/tmp/fndr-testbed-$$.txt"; /usr/sbin/cupsfilter "/tmp/fndr-testbed-$$.txt" > "$1" 2>/dev/null; rm -f "/tmp/fndr-testbed-$$.txt"; }
page "Papers/photosynthesis.pdf" "Photosynthesis turns light, water and carbon dioxide into sugar and oxygen.
$(printf '\f')Chlorophyll absorbs red and blue light and reflects green, which is why leaves look green.
$(printf '\f')In the Calvin cycle the enzyme RuBisCO fixes carbon dioxide."
page "Papers/employment-contract.pdf" "Employment contract. The employee works 40 hours a week.
$(printf '\f')Notice period: either party may end the contract with one month's written notice.
$(printf '\f')Holidays: 25 days a year plus public holidays."

shot() {
  printf '%s' "$2" > "/tmp/fndr-shot-$$.txt"
  /usr/sbin/cupsfilter "/tmp/fndr-shot-$$.txt" > "/tmp/fndr-shot-$$.pdf" 2>/dev/null
  sips -s format png "/tmp/fndr-shot-$$.pdf" --out "Screenshots/$1" >/dev/null
  rm -f "/tmp/fndr-shot-$$.txt" "/tmp/fndr-shot-$$.pdf"
}
shot "Screenshot payment error.png" "Payment declined. Error code E4027: the card issuer refused the charge. Try another card."
shot "Screenshot build failure.png" "error: externally-managed-environment. This environment is externally managed. Use a virtual environment."

say -o "Recordings/sleep-podcast.m4a" --data-format=aac "Welcome back to the show. Today we talk about sleep: why adults need seven to nine hours, and how a dark, cool bedroom helps you fall asleep faster."
say -o "Recordings/cooking-voice-note.m4a" --data-format=aac "Note to self: for the risotto, toast the rice first, then add warm stock one ladle at a time and keep stirring."
say -o "Recordings/directions.m4a" --data-format=aac "Take the second exit at the roundabout, then turn left after the petrol station. The house is the blue one."

echo "Testbed in $OUT: $(find "$OUT" -type f | wc -l | tr -d ' ') files"
