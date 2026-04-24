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
	struct SItem
	{
		int m_Type;
		int m_Id;
		std::vector<int32_t> m_Data;
	};
	std::vector<SItem> GenerateWorkload(int Count, uint32_t Seed)
	{
		std::mt19937 Rng(Seed);
		std::vector<SItem> Items;
		Items.reserve(Count);
		for(int i = 0; i < Count; i++)
		{
			SItem It;
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
	constexpr int ItemsPerSnap = 300;
	constexpr int WARMUP = 2000;
	constexpr int MEASURE = 100000;
	auto Items = GenerateWorkload(ItemsPerSnap, 0xDEADBEEF);
	std::printf("[workload] first 3 items (sanity check):\n");
	for(int i = 0; i < 3 && i < (int)Items.size(); i++)
	{
		const auto &It = Items[i];
		std::printf(" [%d] type=%d id=%d size=%zu data[0..2]=%d,%d,%d\n",
			i, It.m_Type, It.m_Id, It.m_Data.size(),
			It.m_Data[0], It.m_Data[1], It.m_Data[2]);
	}

	auto pBuilder = CSnapshotBuilder_New();
	auto pBuffer = CSnapshotBuffer_New();
	for(int k = 0; k < WARMUP; k++)
	{
		pBuilder->Init(false);
		for(const auto &It : Items)
			pBuilder->NewItem(It.m_Type, It.m_Id,
				rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
		pBuilder->Finish(*pBuffer);
	}
	std::vector<double> vMicros;
	vMicros.reserve(MEASURE);
	for(int k = 0; k < MEASURE; k++)
	{
		auto T0 = std::chrono::steady_clock::now();
		pBuilder->Init(false);
		for(const auto &It : Items)
			pBuilder->NewItem(It.m_Type, It.m_Id,
				rust::Slice<const int32_t>(It.m_Data.data(), It.m_Data.size()));
		int Size = pBuilder->Finish(*pBuffer);
		auto T1 = std::chrono::steady_clock::now();
		(void)Size;
		vMicros.push_back(std::chrono::duration<double, std::micro>(T1 - T0).count());
	}
	PrintResults("build snap (300 items)", vMicros);
	return 0;
}
