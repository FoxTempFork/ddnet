// src/tools/snap_bench.cpp
#include <engine/shared/snapshot.h>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <random>
#include <vector>
namespace
{
	struct Item
	{
		int m_Type;
		int m_Id;
		std::vector<int32_t> m_Data;
	};
	std::vector<Item> GenerateWorkload(int Count, uint32_t Seed)
	{
		std::mt19937 Rng(Seed);
		std::vector<Item> Items;
		Items.reserve(Count);
		for(int i = 0; i < Count; i++)
		{
			Item It;
			It.m_Type = 1 + (int)(Rng() % 15); // valid NETOBJTYPE range
			It.m_Id = i;
			int SizeWords = 4 + (int)(Rng() % 13);
			It.m_Data.resize(SizeWords);
			for(auto &W : It.m_Data)
				W = (int32_t)Rng();
			Items.push_back(std::move(It));
		}
		return Items;
	}
	void PrintResults(const char *pName, const std::vector<double> &vMicros)
	{
		auto vSorted = vMicros;
		std::sort(vSorted.begin(), vSorted.end());
		double Sum = 0;
		for(double V : vSorted)
			Sum += V;
		double Mean = Sum / vSorted.size();
		double Median = vSorted[vSorted.size() / 2];
		double P95 = vSorted[(size_t)(vSorted.size() * 0.95)];
		double P99 = vSorted[(size_t)(vSorted.size() * 0.99)];
		std::printf("=== %s ===\n", pName);
		std::printf(" iterations: %zu\n", vSorted.size());
		std::printf(" mean: %8.3f us\n", Mean);
		std::printf(" median: %8.3f us\n", Median);
		std::printf(" p95: %8.3f us\n", P95);
		std::printf(" p99: %8.3f us\n", P99);
		std::printf(" total: %8.3f ms\n", Sum / 1000.0);
	}
} // namespace
int main()
{
	constexpr int ITEMS_PER_SNAP = 300;
	constexpr int WARMUP = 2000;
	constexpr int MEASURE = 100000;
	auto Items = GenerateWorkload(ITEMS_PER_SNAP, 0xDEADBEEF);
	std::printf("[workload] first 3 items (sanity check):\n");
	for(int i = 0; i < 3 && i < (int)Items.size(); i++)
	{
		const auto &It = Items[i];
		std::printf(" [%d] type=%d id=%d size=%zu data[0..2]=%d,%d,%d\n",
			i, It.m_Type, It.m_Id, It.m_Data.size(),
			It.m_Data[0], It.m_Data[1], It.m_Data[2]);
	}
	// --- pre-Rust C++ variant ---
	// CSnapshotBuilder Builder;
	// CSnapshotBuffer Buffer;
	// Builder.Init();
	// Builder.NewItem(It.m_Type, It.m_Id, It.m_Data.data(),
	// (int)(It.m_Data.size() * sizeof(int32_t)));
	// Builder.Finish(&Buffer);
	// --- current Rust variant ---
	auto pBuilder = CSnapshotBuilder_New();
	auto pBuffer = CSnapshotBuffer_New();

	// Pre-flatten workload for batched Rust call.
	std::vector<int32_t> vTypes;
	std::vector<int32_t> vIds;
	std::vector<int32_t> vFlatData;
	std::vector<uint32_t> vOffsets;
	vTypes.reserve(Items.size());
	vIds.reserve(Items.size());
	vOffsets.reserve(Items.size() + 1);
	vOffsets.push_back(0);
	for(const auto &It : Items)
	{
		vTypes.push_back(It.m_Type);
		vIds.push_back(It.m_Id);
		vFlatData.insert(vFlatData.end(), It.m_Data.begin(), It.m_Data.end());
		vOffsets.push_back((uint32_t)vFlatData.size());
	}
	const auto TypesSlice = rust::Slice<const int32_t>(vTypes.data(), vTypes.size());
	const auto IdsSlice = rust::Slice<const int32_t>(vIds.data(), vIds.size());
	const auto DataSlice = rust::Slice<const int32_t>(vFlatData.data(), vFlatData.size());
	const auto OffsetsSlice = rust::Slice<const uint32_t>(vOffsets.data(), vOffsets.size());

	for(int k = 0; k < WARMUP; k++)
	{
		pBuilder->Init(false);
		for(const auto &It : Items)
			pBuilder->NewItem(It.m_Type, It.m_Id,
				rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
		pBuilder->Finish(*pBuffer);
	}
	for(int k = 0; k < WARMUP; k++)
	{
		pBuilder->Init(false);
		pBuilder->NewItemsFlat(TypesSlice, IdsSlice, DataSlice, OffsetsSlice);
		pBuilder->Finish(*pBuffer);
	}
	std::vector<double> vMicros;
	std::vector<double> vMicrosNewItems;
	std::vector<double> vMicrosFinish;
	vMicros.reserve(MEASURE);
	vMicrosNewItems.reserve(MEASURE);
	vMicrosFinish.reserve(MEASURE);
	for(int k = 0; k < MEASURE; k++)
	{
		auto T0 = std::chrono::steady_clock::now();
		pBuilder->Init(false);
		auto T1 = std::chrono::steady_clock::now();
		for(const auto &It : Items)
			pBuilder->NewItem(It.m_Type, It.m_Id,
				rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
		auto T2 = std::chrono::steady_clock::now();
		int Size = pBuilder->Finish(*pBuffer);
		auto T3 = std::chrono::steady_clock::now();
		(void)Size;
		vMicros.push_back(std::chrono::duration<double, std::micro>(T3 - T0).count());
		vMicrosNewItems.push_back(std::chrono::duration<double, std::micro>(T2 - T1).count());
		vMicrosFinish.push_back(std::chrono::duration<double, std::micro>(T3 - T2).count());
	}
	PrintResults("build snap (300 items, per-item)", vMicros);
	PrintResults(" NewItem loop", vMicrosNewItems);
	PrintResults(" Finish", vMicrosFinish);

	// Batched variant.
	std::vector<double> vMicrosBatch;
	vMicrosBatch.reserve(MEASURE);
	for(int k = 0; k < MEASURE; k++)
	{
		auto T0 = std::chrono::steady_clock::now();
		pBuilder->Init(false);
		pBuilder->NewItemsFlat(TypesSlice, IdsSlice, DataSlice, OffsetsSlice);
		int Size = pBuilder->Finish(*pBuffer);
		auto T1 = std::chrono::steady_clock::now();
		(void)Size;
		vMicrosBatch.push_back(std::chrono::duration<double, std::micro>(T1 - T0).count());
	}
	PrintResults("build snap (300 items, batch)", vMicrosBatch);

	// --- delta bench (create + unpack) ---
	auto pDelta = CSnapshotDelta_New();
	// Register unknown static sizes for a small type range (forces size field in deltas).
	for(int t = 0; t < 64; t++)
		pDelta->SetStaticsize(t, 0);

	auto pFromBuf = CSnapshotBuffer_New();
	auto pToBuf = CSnapshotBuffer_New();
	auto pOutBuf = CSnapshotBuffer_New();

	// Create a slightly modified snapshot for delta workload.
	auto Items2 = Items;
	for(size_t i = 0; i < Items2.size(); i++)
	{
		if((i % 3) == 0 && !Items2[i].m_Data.empty())
			Items2[i].m_Data[0] ^= 0x5a5a5a5a;
	}

	pBuilder->Init(false);
	for(const auto &It : Items)
		pBuilder->NewItem(It.m_Type, It.m_Id, rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
	int FromSize = pBuilder->Finish(*pFromBuf);

	pBuilder->Init(false);
	for(const auto &It : Items2)
		pBuilder->NewItem(It.m_Type, It.m_Id, rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
	int ToSize = pBuilder->Finish(*pToBuf);

	(void)FromSize;
	(void)ToSize;

	std::vector<int32_t> vDeltaInts(CSnapshot::MAX_SIZE / sizeof(int32_t));
	const auto DeltaOut = rust::Slice<int32_t>(vDeltaInts.data(), vDeltaInts.size());

	// Create one delta to size the input slice for UnpackDelta.
	int DeltaBytes = pDelta->CreateDelta(*pFromBuf->AsSnapshot(), *pToBuf->AsSnapshot(), DeltaOut);
	const size_t DeltaLen = DeltaBytes > 0 ? (size_t)DeltaBytes / sizeof(int32_t) : 0;
	const auto DeltaIn = rust::Slice<const int32_t>(vDeltaInts.data(), DeltaLen);

	std::vector<double> vMicrosCreateDelta;
	std::vector<double> vMicrosUnpackDelta;
	vMicrosCreateDelta.reserve(MEASURE);
	vMicrosUnpackDelta.reserve(MEASURE);

	for(int k = 0; k < MEASURE; k++)
	{
		auto T0 = std::chrono::steady_clock::now();
		int Size = pDelta->CreateDelta(*pFromBuf->AsSnapshot(), *pToBuf->AsSnapshot(), DeltaOut);
		auto T1 = std::chrono::steady_clock::now();
		(void)Size;
		vMicrosCreateDelta.push_back(std::chrono::duration<double, std::micro>(T1 - T0).count());
	}

	for(int k = 0; k < MEASURE; k++)
	{
		auto T0 = std::chrono::steady_clock::now();
		int Size = pDelta->UnpackDelta(*pFromBuf->AsSnapshot(), *pOutBuf, DeltaIn);
		auto T1 = std::chrono::steady_clock::now();
		(void)Size;
		vMicrosUnpackDelta.push_back(std::chrono::duration<double, std::micro>(T1 - T0).count());
	}

	PrintResults("create delta (300 items, 1/3 changed)", vMicrosCreateDelta);
	PrintResults("unpack delta (300 items, 1/3 changed)", vMicrosUnpackDelta);
	return 0;
}
